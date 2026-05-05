// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//!
//! End-to-end tests of the `trammel` binary itself: argument parsing, exit
//! codes, --json output, and the `inspect` subcommand. These complement
//! `tests/run.rs` (which exercises the library API) by covering the bin's
//! plumbing — flag wiring, subcommand dispatch, and stdout formatting.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const TRAMMEL_TOML: &str = r#"
src_root = "src"

[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "world"
paths = ["world/**"]

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*"]
rule = "APP_NO_AXUM"
message = "app/ must not import {pattern}"

[[forbidden_constructors]]
in_layers_except = ["world"]
patterns = ["Utc::now"]
position = "any"
rule = "CLOCK_ONLY_IN_WORLD"
message = "`{path}` outside world/"
"#;

/// Build a tempdir laid out like a real project: trammel.toml at root,
/// `src/lib.rs` present (satisfies the implicit lib expectation), plus any
/// extra files the test wants.
fn project(extras: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "trammel.toml", TRAMMEL_TOML);
    write(dir.path(), "src/lib.rs", "");
    for (rel, contents) in extras {
        write(dir.path(), rel, contents);
    }
    dir
}

fn write(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(p) = abs.parent() {
        fs::create_dir_all(p).expect("mkdir");
    }
    fs::write(abs, contents).expect("write");
}

fn trammel(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("trammel").expect("cargo bin");
    cmd.current_dir(dir);
    cmd
}

// ── trammel check (default + --json) ─────────────────────────────────────────

#[test]
fn check_clean_project_exits_zero() {
    let dir = project(&[("src/app/clean.rs", "fn x() {}\n")]);
    trammel(dir.path()).arg("check").assert().success();
}

#[test]
fn check_dirty_project_exits_one_with_human_report() {
    let dir = project(&[("src/app/bad.rs", "use axum::Json;\n")]);
    let out = trammel(dir.path())
        .arg("check")
        .assert()
        .failure()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("APP_NO_AXUM"), "stdout: {s}");
    assert!(s.contains("trammel: FAILED"), "stdout: {s}");
}

#[test]
fn check_json_empty_renders_array_literal() {
    let dir = project(&[("src/app/clean.rs", "fn x() {}\n")]);
    let out = trammel(dir.path())
        .args(["check", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    let parsed: Value = serde_json::from_str(s.trim()).expect("parses");
    assert_eq!(parsed.as_array().expect("array").len(), 0);
}

#[test]
fn check_json_carries_violation_objects() {
    let dir = project(&[
        ("src/app/bad.rs", "use axum::Json;\n"),
        ("src/app/clock.rs", "fn n() -> _ { Utc::now() }\n"),
    ]);
    let out = trammel(dir.path())
        .args(["check", "--json"])
        .assert()
        .failure()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    let parsed: Value = serde_json::from_str(s.trim()).expect("parses");
    let arr = parsed.as_array().expect("array");
    assert!(
        arr.len() >= 2,
        "expected at least 2 violations, got {arr:?}"
    );

    let rules: Vec<&str> = arr
        .iter()
        .map(|v| v["rule"].as_str().expect("rule"))
        .collect();
    assert!(rules.contains(&"APP_NO_AXUM"));
    assert!(rules.contains(&"CLOCK_ONLY_IN_WORLD"));

    // Stable keys present on every entry.
    for v in arr {
        assert!(v.get("file").is_some());
        assert!(v.get("line").is_some());
        assert!(v.get("rule").is_some());
        assert!(v.get("message").is_some());
    }
}

#[test]
fn check_json_does_not_emit_human_report() {
    let dir = project(&[("src/app/bad.rs", "use axum::Json;\n")]);
    let out = trammel(dir.path())
        .args(["check", "--json"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(
        !s.contains("trammel: FAILED"),
        "json mode must not emit the human report; got:\n{s}"
    );
}

// ── trammel inspect ──────────────────────────────────────────────────────────

#[test]
fn inspect_classifies_relative_path_into_layer() {
    let dir = project(&[]);
    let out = trammel(dir.path())
        .args(["inspect", "app/foo.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("layer: app"), "{s}");
    assert!(s.contains("forbidden_imports/APP_NO_AXUM"), "{s}");
}

#[test]
fn inspect_accepts_src_prefixed_path() {
    let dir = project(&[]);
    let out = trammel(dir.path())
        .args(["inspect", "src/app/foo.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("layer: app"), "{s}");
}

#[test]
fn inspect_accepts_absolute_path_under_src_root() {
    let dir = project(&[("src/app/foo.rs", "")]);
    let abs = dir.path().join("src/app/foo.rs");
    let out = trammel(dir.path())
        .args(["inspect", abs.to_str().expect("utf8 path")])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("layer: app"), "{s}");
}

#[test]
fn inspect_unclassified_path_reports_no_layer() {
    let dir = project(&[]);
    let out = trammel(dir.path())
        .args(["inspect", "random/file.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("unclassified"), "{s}");
}

#[test]
fn inspect_absolute_path_outside_src_root_errors() {
    let dir = project(&[]);
    trammel(dir.path())
        .args(["inspect", "/etc/hosts"])
        .assert()
        .failure()
        .code(2);
}

// ── trammel rules ────────────────────────────────────────────────────────────

#[test]
fn rules_list_reports_separate_kinds_for_inline_and_constructors() {
    let dir = project(&[]);
    let out = trammel(dir.path())
        .args(["rules", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("APP_NO_AXUM (forbidden_imports)"), "{s}");
    assert!(
        s.contains("CLOCK_ONLY_IN_WORLD (forbidden_constructors)"),
        "{s}"
    );
}

#[test]
fn rules_explain_prints_message_for_named_rule() {
    let dir = project(&[]);
    let out = trammel(dir.path())
        .args(["rules", "explain", "APP_NO_AXUM"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("rule: APP_NO_AXUM"), "{s}");
    assert!(s.contains("kind: forbidden_imports"), "{s}");
    assert!(s.contains("must not import"), "{s}");
}

#[test]
fn rules_accepts_explicit_config_flag() {
    // Place the config under a different name; verify --config picks it up.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/lib.rs", "");
    write(dir.path(), "alt.toml", TRAMMEL_TOML);
    trammel(dir.path())
        .args(["rules", "--config", "alt.toml", "list"])
        .assert()
        .success();
}

// ── error paths (exit 2) ─────────────────────────────────────────────────────

#[test]
fn check_with_missing_config_exits_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    trammel(dir.path())
        .args(["check", "--config", "does-not-exist.toml"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn check_with_malformed_toml_exits_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/lib.rs", "");
    write(dir.path(), "trammel.toml", "this isn't = valid toml [[");
    trammel(dir.path()).arg("check").assert().failure().code(2);
}

#[test]
fn check_with_invalid_config_semantics_exits_two() {
    // Validates: rule with neither in_layers nor in_files.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/lib.rs", "");
    write(
        dir.path(),
        "trammel.toml",
        r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_imports]]
patterns = ["axum*"]
rule = "BAD"
"#,
    );
    trammel(dir.path()).arg("check").assert().failure().code(2);
}

#[test]
fn rules_with_missing_config_exits_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    trammel(dir.path())
        .args(["rules", "--config", "does-not-exist.toml", "list"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn rules_explain_unknown_rule_exits_two() {
    let dir = project(&[]);
    trammel(dir.path())
        .args(["rules", "explain", "DOES_NOT_EXIST"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn inspect_with_missing_config_exits_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    trammel(dir.path())
        .args(["inspect", "--config", "does-not-exist.toml", "app/foo.rs"])
        .assert()
        .failure()
        .code(2);
}

// ── --src override + comprehensive collect_rules / inspect ───────────────────

const COMPREHENSIVE_TOML: &str = r#"
src_root = "src"

[[layers]]
name = "app"
paths = ["app/**"]
exempt_files = ["app/exempt.rs"]

[[layers]]
name = "system"
paths = ["system/**"]

[[layers]]
name = "tests"
paths = ["tests/**"]
implicit_test_context = true

[[fs_must_exist]]
path = "src/lib.rs"
rule = "LIB_IS_FILE"

[[fs_must_not_exist]]
path = "src/adapters"
rule = "NO_ADAPTERS"

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*"]
rule = "APP_NO_AXUM"

[[forbidden_inline_paths]]
in_layers = ["app"]
patterns = ["db::*"]
position = "any"
rule = "APP_NO_DB"

[[forbidden_constructors]]
in_layers_except = ["system"]
patterns = ["Utc::now"]
position = "any"
rule = "CLOCK_LEAF"

[[forbidden_macros]]
in_layers = ["app"]
qualified_names = ["sqlx::*"]
rule = "APP_NO_SQLX_MACRO"

[[forbidden_methods]]
in_layers = ["app"]
methods = ["unwrap"]
rule = "APP_NO_UNWRAP"
message = "no unwrap please"

[[required_struct_attrs]]
in_layers = ["system"]
struct_name_pattern = "Stub*"
required_any_of = ["cfg(test)"]
rule = "STUBS_GATED"

[[file_content_scan]]
glob = "src/**/*.html"
forbidden_substrings = ["bad-token"]
rule = "TEMPLATE_NO_BAD_TOKEN"

[n_plus_one]
in_layers = ["app"]
db_path_patterns = ["db::*"]
db_macros = ["query"]
combinators = ["map", "for_each"]
opt_out_attribute = "allow_n_plus_one"
rule = "N_PLUS_ONE"
"#;

fn comprehensive_project() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "trammel.toml", COMPREHENSIVE_TOML);
    write(dir.path(), "src/lib.rs", "");
    write(dir.path(), "src/app/foo.rs", "");
    write(dir.path(), "src/app/exempt.rs", "");
    write(dir.path(), "src/tests/it.rs", "");
    dir
}

#[test]
fn rules_list_reports_every_rule_kind() {
    let dir = comprehensive_project();
    let out = trammel(dir.path())
        .args(["rules", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    for needle in [
        "(forbidden_imports)",
        "(forbidden_inline_paths)",
        "(forbidden_constructors)",
        "(forbidden_macros)",
        "(forbidden_methods)",
        "(required_struct_attrs)",
        "(fs_must_exist)",
        "(fs_must_not_exist)",
        "(file_content_scan)",
        "(n_plus_one)",
    ] {
        assert!(s.contains(needle), "missing kind {needle}; got:\n{s}");
    }
}

#[test]
fn rules_explain_default_message_branch() {
    // STUBS_GATED has no `message` field — explain prints "(default)".
    let dir = comprehensive_project();
    let out = trammel(dir.path())
        .args(["rules", "explain", "STUBS_GATED"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("message: (default)"), "{s}");
}

#[test]
fn check_src_override_is_threaded_through() {
    // Point trammel at a fake src_root that doesn't exist; the run still
    // succeeds (no source files to walk) and exits 0.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "trammel.toml", COMPREHENSIVE_TOML);
    write(dir.path(), "src/lib.rs", ""); // satisfies LIB_IS_FILE
    trammel(dir.path())
        .args(["check", "--src", "no-such-src"])
        .assert()
        // src_root override means the per-file walk finds nothing; only
        // fs_must_exist (project-root scoped) still runs and passes.
        .success();
}

#[test]
fn inspect_exempt_file_short_circuits() {
    let dir = comprehensive_project();
    let out = trammel(dir.path())
        .args(["inspect", "app/exempt.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("exempt: yes"), "{s}");
    assert!(
        s.contains("no further rules listed"),
        "exempt branch must short-circuit; got:\n{s}"
    );
}

#[test]
fn inspect_implicit_test_context_layer() {
    let dir = comprehensive_project();
    let out = trammel(dir.path())
        .args(["inspect", "tests/it.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("test_context: yes"), "{s}");
}

#[test]
fn inspect_reports_n_plus_one_in_skipped_for_non_app_layer() {
    // n_plus_one.in_layers = ["app"], so for `system/foo.rs` the rule is
    // listed under "do NOT apply".
    let dir = comprehensive_project();
    write(dir.path(), "src/system/foo.rs", "");
    let out = trammel(dir.path())
        .args(["inspect", "system/foo.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(
        s.contains("n_plus_one/N_PLUS_ONE (layer `system` not in n_plus_one.in_layers)"),
        "{s}"
    );
}

#[test]
fn inspect_reports_n_plus_one_in_applies_for_app_layer() {
    let dir = comprehensive_project();
    let out = trammel(dir.path())
        .args(["inspect", "app/foo.rs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("n_plus_one/N_PLUS_ONE"), "{s}");
}

#[test]
fn inspect_absolute_path_to_nonexistent_file_under_src_root() {
    // Hits the canonicalize fallback in resolve_rel_to_src_root: file is
    // absolute, doesn't exist, so canonicalize() fails and we use the path
    // as-is; the strip_prefix still succeeds because both are anchored at
    // the canonicalized src_root.
    let dir = comprehensive_project();
    let canon_root = dir.path().canonicalize().expect("canon");
    let abs = canon_root.join("src/app/hypothetical.rs");
    let out = trammel(dir.path())
        .args(["inspect", abs.to_str().expect("utf8")])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("utf8");
    assert!(s.contains("layer: app"), "{s}");
}
