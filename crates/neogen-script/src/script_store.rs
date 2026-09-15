//! Script file store: player scripts from a folder (backlog 2.7).
//!
//! The store polls a directory (`scan` once, `refresh` on demand): every
//! regular `*.lua` file at the top level becomes an entry `name → { text,
//! hash, mtime }`. **No watch daemon** — plain polling is enough for the
//! MVP and the Godot bridge (hot-reload in phase 5.4 calls `refresh`).
//!
//! # Host code, not sandbox code
//!
//! This module uses `std::fs` freely: it runs on the **host side**, next
//! to the world driver — the Lua sandbox (2.2) is untouched and scripts
//! still cannot touch the filesystem; the store merely *feeds* script
//! sources into `ScriptHost`.
//!
//! # Failure policy
//!
//! A missing directory is a typed store-level error. Per-file problems
//! (unreadable bytes, vanished between listing and read, metadata failure)
//! never panic and never abort the scan — they land in the scan report's
//! `failed` list with a reason; on `refresh` the previous good version of
//! a script stays loaded. Non-`.lua` files and subdirectories are ignored
//! (no recursion in the MVP).
//!
//! Maps are `BTreeMap`s so enumeration order is deterministic.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use neogen_core::Fnv1a;

/// One loaded script file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredScript {
    /// File name including the `.lua` extension (store key).
    pub name: String,
    /// Full script text.
    pub text: String,
    /// FNV-1a hash of the text (change detection; reuses the core hasher).
    pub hash: u64,
    /// File modification time (hot-reload hints; not part of equality of
    /// content — the hash decides).
    pub mtime: SystemTime,
}

/// A file that failed to load during a scan/refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedFile {
    /// File name that failed.
    pub name: String,
    /// Human-readable reason (read/metadata error text).
    pub reason: String,
}

/// What the last scan/refresh saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// Newly discovered scripts.
    pub added: Vec<String>,
    /// Scripts whose content hash changed.
    pub updated: Vec<String>,
    /// Scripts whose files disappeared.
    pub removed: Vec<String>,
    /// Files that failed to load (old versions, if any, stay loaded).
    pub failed: Vec<FailedFile>,
}

/// Store-level errors (per-file problems go to [`ScanReport::failed`]).
#[derive(Debug)]
pub enum StoreError {
    /// The store directory does not exist.
    DirectoryMissing {
        /// The path that was requested.
        path: PathBuf,
    },
    /// Reading the directory failed for another reason (permissions, …).
    Io {
        /// The path that was requested.
        path: PathBuf,
        /// The io error message.
        message: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryMissing { path } => {
                write!(f, "script directory does not exist: {}", path.display())
            }
            Self::Io { path, message } => {
                write!(
                    f,
                    "cannot read script directory {}: {message}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// A successfully loaded entry during a directory pass.
struct LoadedEntry {
    name: String,
    text: String,
    hash: u64,
    mtime: SystemTime,
}

/// Directory of player scripts, scanned on demand.
#[derive(Debug)]
pub struct ScriptStore {
    dir: PathBuf,
    scripts: BTreeMap<String, StoredScript>,
    last_report: ScanReport,
}

impl ScriptStore {
    /// Scan a directory for the first time. Per-file failures are reported
    /// via [`ScriptStore::last_report`], not through the `Result` (only a
    /// missing directory is a store-level error).
    pub fn scan(dir: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let dir = dir.into();
        let (loaded, failed) = read_entries(&dir)?;
        let mut scripts = BTreeMap::new();
        let mut added = Vec::new();
        for entry in loaded {
            added.push(entry.name.clone());
            scripts.insert(
                entry.name.clone(),
                StoredScript {
                    name: entry.name,
                    text: entry.text,
                    hash: entry.hash,
                    mtime: entry.mtime,
                },
            );
        }
        Ok(Self {
            dir,
            scripts,
            last_report: sorted(ScanReport {
                added,
                updated: Vec::new(),
                removed: Vec::new(),
                failed,
            }),
        })
    }

    /// Re-scan the directory: changed content (by hash) is reloaded, gone
    /// files are dropped, per-file failures keep the previous version.
    /// Returns the diff report (also kept for [`last_report`]).
    pub fn refresh(&mut self) -> Result<ScanReport, StoreError> {
        let (loaded, failed) = read_entries(&self.dir)?;

        let mut report = ScanReport {
            failed,
            ..ScanReport::default()
        };
        let mut fresh: BTreeMap<String, StoredScript> = BTreeMap::new();
        for entry in loaded {
            let stored = StoredScript {
                name: entry.name.clone(),
                text: entry.text,
                hash: entry.hash,
                mtime: entry.mtime,
            };
            match self.scripts.get(&entry.name) {
                None => {
                    report.added.push(entry.name.clone());
                    fresh.insert(entry.name, stored);
                }
                Some(old) if old.hash != entry.hash => {
                    report.updated.push(entry.name.clone());
                    fresh.insert(entry.name, stored);
                }
                Some(_) => {
                    // Content identical: keep the already-loaded text (and
                    // its identity), just refresh the mtime.
                    let mut kept = self.scripts.get(&entry.name).expect("checked").clone();
                    kept.mtime = stored.mtime;
                    fresh.insert(entry.name, kept);
                }
            }
        }
        // Entries present before but neither loaded nor failed now → gone.
        for name in self.scripts.keys() {
            if !fresh.contains_key(name) && !report.failed.iter().any(|f| &f.name == name) {
                report.removed.push(name.clone());
            }
        }
        // Failed files keep their previous good version loaded.
        for failed_file in &report.failed {
            if let Some(old) = self.scripts.get(&failed_file.name) {
                fresh.insert(failed_file.name.clone(), old.clone());
            }
        }

        self.scripts = fresh;
        let report = sorted(report);
        self.last_report = report.clone();
        Ok(report)
    }

    /// Look up a loaded script by file name.
    pub fn get(&self, name: &str) -> Option<&StoredScript> {
        self.scripts.get(name)
    }

    /// All loaded script names, sorted (deterministic).
    pub fn names(&self) -> Vec<String> {
        self.scripts.keys().cloned().collect()
    }

    /// The watched directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The report of the last scan/refresh.
    pub fn last_report(&self) -> &ScanReport {
        &self.last_report
    }

    /// Number of loaded scripts.
    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    /// Whether nothing is loaded.
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

/// Sort every list of a report by name: `read_dir` order is
/// filesystem-dependent, reports must be deterministic (FIX-раунд фазы 2
/// #6).
fn sorted(mut report: ScanReport) -> ScanReport {
    report.added.sort();
    report.updated.sort();
    report.removed.sort();
    report.failed.sort_by(|a, b| a.name.cmp(&b.name));
    report
}

/// One pass over the directory: load every regular `*.lua` file.
/// Per-file problems become [`FailedFile`]s; a missing directory is a
/// store-level error.
fn read_entries(dir: &Path) -> Result<(Vec<LoadedEntry>, Vec<FailedFile>), StoreError> {
    let entries = fs::read_dir(dir).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => StoreError::DirectoryMissing {
            path: dir.to_path_buf(),
        },
        kind => StoreError::Io {
            path: dir.to_path_buf(),
            message: format!("{kind}: {error}"),
        },
    })?;

    let mut loaded = Vec::new();
    let mut failed = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failed.push(FailedFile {
                    name: "<directory listing>".to_string(),
                    reason: error.to_string(),
                });
                continue;
            }
        };
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue, // non-UTF-8 file name: skip silently
        };
        if path.extension().and_then(|e| e.to_str()) != Some("lua") {
            continue; // non-lua: ignored
        }
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue; // directories (even *.lua-named) and specials: ignored
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                let mtime = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let mut hasher = Fnv1a::new();
                for byte in text.bytes() {
                    hasher.write_u8(byte);
                }
                loaded.push(LoadedEntry {
                    name,
                    text,
                    hash: hasher.finish(),
                    mtime,
                });
            }
            // The file can vanish between listing and read, or hold
            // non-UTF-8 bytes: record, never panic.
            Err(error) => failed.push(FailedFile {
                name,
                reason: error.to_string(),
            }),
        }
    }
    Ok((loaded, failed))
}
