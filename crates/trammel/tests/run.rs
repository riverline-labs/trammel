// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//!
//! End-to-end test: build a small project tree on disk, run `trammel::run`,
//! and assert on the aggregated violations. Exercises the full pipeline
//! (fs_layout → file_content_scan → per-file AST walk) in one shot.

use std::fs;
use std::path::Path;

use trammel::config::Config;
use trammel::run;

const TRAMMEL_TOML: &str = r#"
src_root = "src"

[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[layers]]
name = "db"
paths = ["db/**"]

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*"]
rule = "APP_NO_AXUM"
message = "app/ must not import {pattern}"

[[forbidden_methods]]
in_layers = ["app"]
methods = ["unwrap"]
allow_in_test_context = true
rule = "NO_UNWRAP"

[[fs_must_exist]]
path = "src/lib.rs"
rule = "LIB_IS_FILE"

[[fs_must_not_exist]]
paths = ["src/adapters"]
rule = "FORBIDDEN_PATHS"

[[file_content_scan]]
glob = "src/transports/web/templates/**"
forbidden_substrings = ["session.persona"]
rule = "TEMPLATE_NO_PERSONA_CHECK"

[n_plus_one]
in_layers = ["app", "db"]
db_path_patterns = ["db", "db::*", "crate::db*"]
db_macros = ["query"]
combinators = ["for_each", "then"]
opt_out_attribute = "allow_n_plus_one"
layer_assumes_query = ["db"]
rule = "N_PLUS_ONE"
"#;

fn write(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(p) = abs.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(abs, contents).unwrap();
}

fn cfg() -> Config {
    toml::from_str(TRAMMEL_TOML).unwrap()
}

#[test]
fn ok_project_produces_no_violations() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/lib.rs", "");
    write(dir.path(), "src/app/clean.rs", "fn x() { let _ = 1; }\n");
    write(
        dir.path(),
        "src/transports/web/router.rs",
        "fn route() {}\n",
    );

    let v = run(&cfg(), dir.path()).unwrap();
    assert!(v.is_empty(), "expected no violations: {v:?}");
}

#[test]
fn fail_project_aggregates_violations_across_passes() {
    let dir = tempfile::tempdir().unwrap();

    // Missing src/lib.rs → LIB_IS_FILE
    // Present src/adapters → FORBIDDEN_PATHS
    fs::create_dir_all(dir.path().join("src/adapters")).unwrap();

    // app/ imports axum → APP_NO_AXUM
    // app/ uses .unwrap() outside test → NO_UNWRAP
    write(
        dir.path(),
        "src/app/handler.rs",
        r#"
        use axum::http::StatusCode;
        fn x(r: Result<u32, ()>) {
            let _ = r.unwrap();
        }
        "#,
    );

    // template contains forbidden substring → TEMPLATE_NO_PERSONA_CHECK
    write(
        dir.path(),
        "src/transports/web/templates/profile.html",
        "<div>{{ session.persona }}</div>",
    );

    // db/ + loop + await → N_PLUS_ONE (layer_assumes_query)
    write(
        dir.path(),
        "src/db/users.rs",
        r#"
        async fn batch(ids: Vec<u32>) {
            for _ in ids {
                let _ = some_async().await;
            }
        }
        "#,
    );

    let v = run(&cfg(), dir.path()).unwrap();
    let rules: Vec<&str> = v.iter().map(|x| x.rule.as_str()).collect();

    assert!(rules.contains(&"LIB_IS_FILE"), "{rules:?}");
    assert!(rules.contains(&"FORBIDDEN_PATHS"), "{rules:?}");
    assert!(rules.contains(&"APP_NO_AXUM"), "{rules:?}");
    assert!(rules.contains(&"NO_UNWRAP"), "{rules:?}");
    assert!(rules.contains(&"TEMPLATE_NO_PERSONA_CHECK"), "{rules:?}");
    assert!(rules.contains(&"N_PLUS_ONE"), "{rules:?}");
}

#[test]
fn exempt_files_are_skipped_even_when_content_would_fire() {
    // visitor::check_file early-returns on layer_set.is_exempt — without
    // this test, that branch was unexercised (the inspect short-circuit
    // fires earlier and never reaches the visitor).
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/lib.rs", "");
    let toml = r#"
src_root = "src"

[[layers]]
name = "app"
paths = ["app/**"]
exempt_files = ["app/legacy.rs"]

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*"]
rule = "APP_NO_AXUM"
"#;
    write(dir.path(), "src/app/legacy.rs", "use axum::http::StatusCode;\n");
    write(dir.path(), "src/app/clean.rs", "fn x() {}\n");
    let cfg: Config = toml::from_str(toml).unwrap();
    let v = run(&cfg, dir.path()).unwrap();
    assert!(
        v.iter().all(|x| x.rule != "APP_NO_AXUM"),
        "exempt file must not be visited: {v:?}"
    );
}

#[test]
fn skips_files_outside_any_layer() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "src/lib.rs", "");
    // unclassified path — must not trigger app's forbidden_imports
    write(
        dir.path(),
        "src/random/foo.rs",
        "use axum::http::StatusCode;\n",
    );

    let v = run(&cfg(), dir.path()).unwrap();
    assert!(
        v.iter().all(|x| x.rule != "APP_NO_AXUM"),
        "unclassified file should not trigger app rules: {v:?}"
    );
}
