//! Backlog 2.7 — script store over a folder.

use std::fs;
use std::path::PathBuf;

use neogen_core::World;
use neogen_script::{
    RuntimeConfig, ScriptHost, ScriptState, ScriptStore, StoreError, WorldContext,
};

/// Temp directory removed on drop; unique per test via `tag` + pid.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("neogen-script-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir creates");
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) {
        fs::write(self.0.join(name), contents).expect("write file");
    }

    fn write_bytes(&self, name: &str, contents: &[u8]) {
        fs::write(self.0.join(name), contents).expect("write file");
    }

    fn remove(&self, name: &str) {
        fs::remove_file(self.0.join(name)).expect("remove file");
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn valid_broken_and_foreign_files() {
    let dir = TempDir::new("mixed");
    dir.write("good.lua", "print('hello')");
    // Invalid UTF-8: a read failure, recorded as failed.
    dir.write_bytes("broken.lua", &[0xFF, 0xFE, 0x00]);
    dir.write("notes.txt", "not a script");
    fs::create_dir_all(dir.path().join("subdir.lua")).expect("dir");

    let store = ScriptStore::scan(dir.path()).expect("scan works");
    assert_eq!(store.names(), vec!["good.lua".to_string()]);
    let good = store.get("good.lua").expect("loaded");
    assert_eq!(good.text, "print('hello')");

    let report = store.last_report();
    assert_eq!(report.added, vec!["good.lua".to_string()]);
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.failed[0].name, "broken.lua");
    assert!(!report.failed[0].reason.is_empty());
}

#[test]
fn refresh_reports_updates_with_new_text() {
    let dir = TempDir::new("update");
    dir.write("a.lua", "return 1");

    let mut store = ScriptStore::scan(dir.path()).expect("scan");
    let first_hash = store.get("a.lua").expect("loaded").hash;

    dir.write("a.lua", "return 2");
    let report = store.refresh().expect("refresh");

    assert_eq!(report.added, Vec::<String>::new());
    assert_eq!(report.updated, vec!["a.lua".to_string()]);
    assert_eq!(report.removed, Vec::<String>::new());
    assert_eq!(store.get("a.lua").expect("loaded").text, "return 2");
    assert_ne!(store.get("a.lua").expect("loaded").hash, first_hash);
}

#[test]
fn unchanged_files_are_not_updated() {
    let dir = TempDir::new("stable");
    dir.write("a.lua", "return 1");
    let mut store = ScriptStore::scan(dir.path()).expect("scan");
    let report = store.refresh().expect("refresh");
    assert!(report.added.is_empty() && report.updated.is_empty() && report.removed.is_empty());
}

#[test]
fn refresh_reports_removals() {
    let dir = TempDir::new("remove");
    dir.write("a.lua", "return 1");
    dir.write("b.lua", "return 2");
    let mut store = ScriptStore::scan(dir.path()).expect("scan");
    assert_eq!(store.len(), 2);

    dir.remove("a.lua");
    let report = store.refresh().expect("refresh");
    assert_eq!(report.removed, vec!["a.lua".to_string()]);
    assert!(store.get("a.lua").is_none());
    assert_eq!(store.names(), vec!["b.lua".to_string()]);
}

#[test]
fn refresh_reports_additions() {
    let dir = TempDir::new("add");
    dir.write("a.lua", "return 1");
    let mut store = ScriptStore::scan(dir.path()).expect("scan");
    dir.write("new.lua", "return 3");
    let report = store.refresh().expect("refresh");
    assert_eq!(report.added, vec!["new.lua".to_string()]);
    assert_eq!(store.len(), 2);
}

#[test]
fn failed_refresh_keeps_the_previous_version() {
    let dir = TempDir::new("failed-keep");
    dir.write("a.lua", "return 1");
    let mut store = ScriptStore::scan(dir.path()).expect("scan");

    // Corrupt the file (invalid UTF-8): refresh must fail it, not unload it.
    dir.write_bytes("a.lua", &[0xFF, 0xFF]);
    let report = store.refresh().expect("refresh");
    assert_eq!(report.updated, Vec::<String>::new());
    assert_eq!(report.failed.len(), 1);
    assert_eq!(store.get("a.lua").expect("kept").text, "return 1");
}

#[test]
fn missing_directory_is_a_typed_error() {
    let dir = TempDir::new("missing-target");
    let nowhere = dir.path().join("does-not-exist");
    match ScriptStore::scan(&nowhere) {
        Err(StoreError::DirectoryMissing { path }) => assert_eq!(path, nowhere),
        other => panic!("expected DirectoryMissing, got {other:?}"),
    }
    let mut store = ScriptStore::scan(TempDir::new("missing-src").path()).expect("scan");
    assert!(matches!(
        store.refresh(),
        Err(StoreError::DirectoryMissing { .. })
    ));
}

#[test]
fn attach_from_store_runs_the_script() {
    let dir = TempDir::new("attach");
    dir.write("drive.lua", "print('from store') move(1, 2)");
    let store = ScriptStore::scan(dir.path()).expect("scan");

    let mut host = ScriptHost::new(World::new(5), RuntimeConfig::default()).expect("host");
    let rover = host.rover_ids()[0];
    let id = host
        .attach_from_store(rover, &store, "drive.lua")
        .expect("attaches");
    for _ in 0..30 {
        host.step_world();
    }
    assert_eq!(host.script_state(id), Some(ScriptState::Finished));
    assert!(host.log_snapshot().iter().any(|e| e.text == "from store"));
    let position = host.rover_position(rover).expect("rover");
    assert!(position.distance_to(neogen_core::Vec2::new(1.0, 2.0)) < 1e-9);

    // Unknown name: typed host-side error with the store dir in it.
    match host.attach_from_store(rover, &store, "nope.lua") {
        Err(neogen_script::ScriptError::Runtime { message, .. }) => {
            assert!(message.contains("nope.lua"), "{message}");
        }
        other => panic!("expected Runtime error, got {other:?}"),
    }
}

#[test]
fn world_context_direct_use_is_unaffected() {
    // Store code stays out of the world context (host-side separation).
    let context = WorldContext::new(World::new(1));
    assert_eq!(context.world().tick(), 0);
}

// ---- FIX-раунд фазы 2 B: #6 deterministic reports ----

#[test]
fn reports_are_sorted_by_name() {
    let dir = TempDir::new("sorted");
    dir.write("c.lua", "return 3");
    dir.write("a.lua", "return 1");
    dir.write("b.lua", "return 2");
    let mut store = ScriptStore::scan(dir.path()).expect("scan");
    assert_eq!(
        store.last_report().added,
        vec![
            "a.lua".to_string(),
            "b.lua".to_string(),
            "c.lua".to_string()
        ]
    );

    dir.write("a.lua", "return 11");
    dir.remove("b.lua");
    dir.write("d.lua", "return 4");
    let report = store.refresh().expect("refresh");
    assert_eq!(report.updated, vec!["a.lua".to_string()]);
    assert_eq!(report.removed, vec!["b.lua".to_string()]);
    assert_eq!(report.added, vec!["d.lua".to_string()]);
}
