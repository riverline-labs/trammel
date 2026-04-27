// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//!
//! End-to-end rule tests. Each test parses a TOML config and an inline Rust
//! snippet, runs the visitor, and asserts on the produced violations.
//! These exercise the full dispatch path: scope matching, glob matching,
//! visitor propagation, message formatting.

use std::path::Path;

use trammel::config::{self, Config};
use trammel::layers::LayerSet;
use trammel::rules::CompiledRules;
use trammel::violations::Violation;
use trammel::visitor;

fn check(toml: &str, rel_path: &str, src: &str) -> Vec<Violation> {
    let cfg: Config = toml::from_str(toml).expect("config parses");
    config::validate(&cfg).expect("config validates");
    let layer_set = LayerSet::build(&cfg).expect("layer set builds");
    let compiled = CompiledRules::build(&cfg).expect("rules compile");
    let layer = layer_set
        .classify(rel_path)
        .expect("rel_path matches a layer");
    let ast = syn::parse_str::<syn::File>(src).expect("rust parses");
    let path = Path::new(rel_path);
    let mut violations = Vec::new();
    visitor::check_file(
        &cfg,
        &layer_set,
        &compiled,
        path,
        rel_path,
        layer,
        &ast,
        &mut violations,
    );
    violations
}

fn rules_in(violations: &[Violation]) -> Vec<&str> {
    violations.iter().map(|v| v.rule.as_str()).collect()
}

// ── forbidden_imports ────────────────────────────────────────────────────────

const APP_BOUNDARY_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*", "*transports*", "sqlx*"]
rule = "APP_BOUNDARY"
message = "app/ must not import {pattern}"
"#;

#[test]
fn forbidden_imports_clean_app_passes() {
    let v = check(
        APP_BOUNDARY_TOML,
        "app/clean.rs",
        r#"
        use std::collections::HashMap;
        use serde::Serialize;
        use crate::system::clock;
        "#,
    );
    assert!(v.is_empty(), "expected no violations, got {v:?}");
}

#[test]
fn forbidden_imports_app_uses_axum_fails() {
    let v = check(
        APP_BOUNDARY_TOML,
        "app/handler.rs",
        "use axum::http::StatusCode;",
    );
    assert_eq!(rules_in(&v), vec!["APP_BOUNDARY"]);
    assert!(v[0].message.contains("axum*"));
}

#[test]
fn forbidden_imports_app_uses_transports_fails() {
    let v = check(
        APP_BOUNDARY_TOML,
        "app/foo.rs",
        "use crate::transports::web::router;",
    );
    assert_eq!(rules_in(&v), vec!["APP_BOUNDARY"]);
}

#[test]
fn forbidden_imports_group_expands_each_path() {
    // axum::Json triggers; std::fmt does not.
    let v = check(
        APP_BOUNDARY_TOML,
        "app/foo.rs",
        "use axum::{Json, extract::Path};",
    );
    assert_eq!(v.len(), 2);
    assert!(v.iter().all(|x| x.rule == "APP_BOUNDARY"));
}

#[test]
fn forbidden_imports_does_not_fire_outside_layer() {
    // transports_web is not in `in_layers = ["app"]`.
    let v = check(
        APP_BOUNDARY_TOML,
        "transports/web/router.rs",
        "use axum::http::StatusCode;",
    );
    assert!(v.is_empty(), "expected no violations, got {v:?}");
}

#[test]
fn forbidden_imports_allow_in_test_context_skips() {
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["sqlx*"]
rule = "APP_NO_SQLX"
allow_in_test_context = true
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        #[cfg(test)]
        mod tests {
            use sqlx::query;
        }
        "#,
    );
    assert!(v.is_empty(), "test-context import should be allowed: {v:?}");
}

// ── forbidden_inline_paths ───────────────────────────────────────────────────

const TRANSPORTS_NO_DB_TOML: &str = r#"
[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[forbidden_inline_paths]]
in_layers = ["transports_web"]
patterns = ["db", "db::*", "crate::db*"]
position = "any"
rule = "TRANSPORTS_NO_DB"
allow_in_test_context = true
"#;

#[test]
fn forbidden_inline_paths_expr_position_fails() {
    let v = check(
        TRANSPORTS_NO_DB_TOML,
        "transports/web/router.rs",
        r#"
        fn handler() {
            let _ = crate::db::find(id);
        }
        "#,
    );
    assert!(
        v.iter().any(|x| x.rule == "TRANSPORTS_NO_DB"),
        "expected TRANSPORTS_NO_DB, got {v:?}"
    );
}

#[test]
fn forbidden_inline_paths_type_position_fails() {
    let v = check(
        TRANSPORTS_NO_DB_TOML,
        "transports/web/router.rs",
        r#"
        fn handler(u: db::User) {
        }
        "#,
    );
    assert!(v.iter().any(|x| x.rule == "TRANSPORTS_NO_DB"));
}

#[test]
fn forbidden_inline_paths_position_expr_only_does_not_fire_on_type() {
    let toml = r#"
[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[forbidden_inline_paths]]
in_files = ["transports/web/router.rs"]
patterns = ["app::*"]
position = "expr"
rule = "ROUTER_NO_APP"
"#;
    let v = check(
        toml,
        "transports/web/router.rs",
        r#"
        fn handler(_: app::User) {}
        "#,
    );
    assert!(
        v.is_empty(),
        "type-position match must be ignored when position=expr: {v:?}"
    );
}

#[test]
fn forbidden_inline_paths_position_expr_fires_on_expr() {
    let toml = r#"
[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[forbidden_inline_paths]]
in_files = ["transports/web/router.rs"]
patterns = ["app::*"]
position = "expr"
rule = "ROUTER_NO_APP"
"#;
    let v = check(
        toml,
        "transports/web/router.rs",
        r#"
        fn handler() {
            let _ = app::deals::list;
        }
        "#,
    );
    assert!(v.iter().any(|x| x.rule == "ROUTER_NO_APP"));
}

#[test]
fn forbidden_inline_paths_test_context_is_exempt() {
    let v = check(
        TRANSPORTS_NO_DB_TOML,
        "transports/web/router.rs",
        r#"
        #[test]
        fn t() {
            let _ = crate::db::User { name: "x" };
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "test-context inline path should be allowed: {v:?}"
    );
}

// ── forbidden_macros ─────────────────────────────────────────────────────────

const NO_SQL_OUTSIDE_DB_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[layers]]
name = "db"
paths = ["db/**"]

[[forbidden_macros]]
in_layers = ["app", "transports_web"]
qualified_names = ["sqlx::*"]
bare_names = ["query", "query_as"]
bare_names_in_layers = ["app"]
rule = "NO_SQL_OUTSIDE_DB"
allow_in_test_context = true
"#;

#[test]
fn forbidden_macros_qualified_match_fires() {
    let v = check(
        NO_SQL_OUTSIDE_DB_TOML,
        "app/foo.rs",
        r#"
        fn x() {
            sqlx::query!("SELECT 1");
        }
        "#,
    );
    assert!(v.iter().any(|x| x.rule == "NO_SQL_OUTSIDE_DB"));
}

#[test]
fn forbidden_macros_bare_name_in_app_fires() {
    let v = check(
        NO_SQL_OUTSIDE_DB_TOML,
        "app/foo.rs",
        r#"
        fn x() {
            query!("SELECT 1");
        }
        "#,
    );
    assert!(v.iter().any(|x| x.rule == "NO_SQL_OUTSIDE_DB"));
}

#[test]
fn forbidden_macros_bare_name_outside_layer_does_not_fire() {
    // transports_web is in `in_layers` but NOT in `bare_names_in_layers`.
    let v = check(
        NO_SQL_OUTSIDE_DB_TOML,
        "transports/web/handler.rs",
        r#"
        fn x() {
            query!("SELECT 1");
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "bare query!() in transports_web should not fire: {v:?}"
    );
}

#[test]
fn forbidden_macros_db_layer_clean() {
    let v = check(
        NO_SQL_OUTSIDE_DB_TOML,
        "db/users.rs",
        r#"
        fn fetch() {
            sqlx::query!("SELECT 1");
        }
        "#,
    );
    assert!(v.is_empty(), "db/ is not in in_layers: {v:?}");
}

// ── forbidden_methods ────────────────────────────────────────────────────────

const NO_UNWRAP_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_methods]]
in_layers = ["app"]
methods = ["unwrap", "expect"]
allow_in_test_context = true
rule = "NO_UNWRAP_IN_PRODUCTION"
"#;

#[test]
fn forbidden_methods_unwrap_in_prod_fires() {
    let v = check(
        NO_UNWRAP_TOML,
        "app/foo.rs",
        r#"
        fn x() {
            let _ = some_result.unwrap();
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["NO_UNWRAP_IN_PRODUCTION"]);
}

#[test]
fn forbidden_methods_unwrap_in_test_is_exempt() {
    let v = check(
        NO_UNWRAP_TOML,
        "app/foo.rs",
        r#"
        #[test]
        fn t() {
            let _ = some_result.unwrap();
        }
        "#,
    );
    assert!(v.is_empty(), "unwrap in #[test] should be allowed: {v:?}");
}

#[test]
fn forbidden_methods_other_method_does_not_fire() {
    let v = check(
        NO_UNWRAP_TOML,
        "app/foo.rs",
        r#"
        fn x() {
            let _ = some_result.ok();
        }
        "#,
    );
    assert!(v.is_empty(), "`.ok()` is not in `methods`: {v:?}");
}
