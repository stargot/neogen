use neogen_core::engine_version;

#[test]
fn engine_version_exposes_crate_version() {
    let version = engine_version();
    assert!(!version.is_empty());
    // Semantic version shape: major.minor(.patch).
    let parts: Vec<_> = version.split('.').collect();
    assert!(
        parts.len() >= 2,
        "expected at least major.minor, got {version}"
    );
    assert!(
        parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "expected numeric version components, got {version}"
    );
}
