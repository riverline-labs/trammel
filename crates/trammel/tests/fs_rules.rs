// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//!
//! Filesystem-only rule tests (`fs_must_exist`, `fs_must_not_exist`,
//! `file_content_scan`). Each test builds a small directory tree under a
//! tempdir and asserts on produced violations.

use std::fs;
use std::path::Path;

use trammel::config::Config;
use trammel::rules::{file_content_scan, fs_layout};
use trammel::violations::Violation;

fn parse(toml_str: &str) -> Config {
    toml::from_str(toml_str).expect("config parses")
}

fn write(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(abs, contents).unwrap();
}

fn rules_in(violations: &[Violation]) -> Vec<&str> {
    violations.iter().map(|v| v.rule.as_str()).collect()
}

// ── fs_must_exist ────────────────────────────────────────────────────────────

#[test]
fn fs_must_exist_passes_when_present() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/lib.rs", "");
    let cfg = parse(
        r#"
[[fs_must_exist]]
path = "src/lib.rs"
rule = "LIB_IS_FILE"
"#,
    );
    let mut v = Vec::new();
    fs_layout::check(&cfg, dir.path(), &mut v);
    assert!(v.is_empty(), "{v:?}");
}

#[test]
fn fs_must_exist_fires_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = parse(
        r#"
[[fs_must_exist]]
path = "src/lib.rs"
rule = "LIB_IS_FILE"
message = "library root must be src/lib.rs"
"#,
    );
    let mut v = Vec::new();
    fs_layout::check(&cfg, dir.path(), &mut v);
    assert_eq!(rules_in(&v), vec!["LIB_IS_FILE"]);
    assert!(v[0].message.contains("library root"));
    assert_eq!(v[0].line, 0);
}

// ── fs_must_not_exist ────────────────────────────────────────────────────────

#[test]
fn fs_must_not_exist_passes_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = parse(
        r#"
[[fs_must_not_exist]]
paths = ["src/adapters", "src/app_state.rs"]
rule = "FORBIDDEN_PATHS"
"#,
    );
    let mut v = Vec::new();
    fs_layout::check(&cfg, dir.path(), &mut v);
    assert!(v.is_empty(), "{v:?}");
}

#[test]
fn fs_must_not_exist_fires_for_each_present_path() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/app_state.rs", "");
    fs::create_dir_all(dir.path().join("src/adapters")).unwrap();
    let cfg = parse(
        r#"
[[fs_must_not_exist]]
paths = ["src/adapters", "src/app_state.rs"]
rule = "FORBIDDEN_PATHS"
"#,
    );
    let mut v = Vec::new();
    fs_layout::check(&cfg, dir.path(), &mut v);
    assert_eq!(v.len(), 2);
    assert!(v.iter().all(|x| x.rule == "FORBIDDEN_PATHS"));
}

#[test]
fn fs_must_not_exist_accepts_singular_path_field() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/lib.rs", "");
    let cfg = parse(
        r#"
[[fs_must_not_exist]]
path = "src/lib.rs"
rule = "LIB_IS_FILE_DIR"
"#,
    );
    let mut v = Vec::new();
    fs_layout::check(&cfg, dir.path(), &mut v);
    assert_eq!(rules_in(&v), vec!["LIB_IS_FILE_DIR"]);
}

// ── file_content_scan ────────────────────────────────────────────────────────

#[test]
fn file_content_scan_finds_substring() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "src/transports/web/templates/profile.html",
        "<div>{{ session.persona }}</div>",
    );
    write(
        dir.path(),
        "src/transports/web/templates/clean.html",
        "<div>hello</div>",
    );
    let cfg = parse(
        r#"
[[file_content_scan]]
glob = "src/transports/web/templates/**"
forbidden_substrings = ["session.persona"]
rule = "TEMPLATE_NO_PERSONA_CHECK"
"#,
    );
    let mut v = Vec::new();
    file_content_scan::check(&cfg, dir.path(), &mut v).unwrap();
    assert_eq!(rules_in(&v), vec!["TEMPLATE_NO_PERSONA_CHECK"]);
    assert!(v[0].file.ends_with("profile.html"));
}

#[test]
fn file_content_scan_respects_exclude_glob() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/transports/web/handler.rs", ".is_admin()");
    write(
        dir.path(),
        "src/transports/web/templates/foo.html",
        ".is_admin()",
    );
    let cfg = parse(
        r#"
[[file_content_scan]]
glob = "src/transports/**/*.rs"
exclude_glob = "src/transports/web/templates/**"
forbidden_substrings = [".is_admin()"]
rule = "TRANSPORT_NO_IS_ADMIN"
"#,
    );
    let mut v = Vec::new();
    file_content_scan::check(&cfg, dir.path(), &mut v).unwrap();
    assert_eq!(rules_in(&v), vec!["TRANSPORT_NO_IS_ADMIN"]);
    assert!(v[0].file.ends_with("handler.rs"));
}

#[test]
fn file_content_scan_no_match_clean() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "src/transports/web/templates/clean.html",
        "<div>hello</div>",
    );
    let cfg = parse(
        r#"
[[file_content_scan]]
glob = "src/transports/web/templates/**"
forbidden_substrings = ["session.persona"]
rule = "TEMPLATE_NO_PERSONA_CHECK"
"#,
    );
    let mut v = Vec::new();
    file_content_scan::check(&cfg, dir.path(), &mut v).unwrap();
    assert!(v.is_empty(), "{v:?}");
}
