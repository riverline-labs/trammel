# Trammel Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `trammel` — a config-driven Rust architectural conformance tool. Extract the rule engine from north-star's `arch-lint` into a standalone Apache 2.0 crate owned by Riverline Labs LLC, publish to crates.io, then migrate north-star to consume it.

**Architecture:** Two crates published from one repo. `trammel` (lib + bin) parses Rust source with `syn`, classifies files into layers per `trammel.toml`, dispatches each AST node to the matching rule kinds. `trammel-attrs` is a small proc-macro crate exporting `#[allow_n_plus_one]` (a no-op marker the engine reads from source).

**Tech Stack:** Rust stable (MSRV 1.83). Dependencies: `syn` (full + visit + extra-traits with span-locations from `proc-macro2`), `walkdir`, `globset`, `serde`, `toml`, `anyhow`, `clap` (CLI binary), `quote` (proc-macro). No nightly features, no rustc-internal APIs.

**Reference spec:** `2026-04-27-trammel-design.md` (this same `docs/` folder).

**Reference source (for porting):** `~/src/edms/north-star/crates/arch-lint/src/main.rs` (1,020 lines) and `~/src/edms/north-star/crates/arch-lint-attrs/src/lib.rs` (20 lines). Read these alongside the spec as you implement — every behavior in the source must end up reproducible from the trammel config.

**Non-goals (re-affirmed from spec):** Editor / LSP integration, user-defined Rust rules, severity levels, line-level suppression comments, multi-file config layering.

---

## File Structure

### Repo: `~/src/rll/trammel/` (new) → `github.com/riverline-labs/trammel`

```
trammel/
├── Cargo.toml                                      workspace
├── LICENSE                                         Apache 2.0
├── NOTICE                                          Copyright (c) 2026 Riverline Labs LLC
├── README.md                                       usage, config schema, rule kinds
├── CHANGELOG.md                                    keep-a-changelog format
├── .gitignore                                      target/, *.swp, etc.
├── .github/workflows/ci.yml                        fmt + clippy + test on stable; matrix linux + macos
├── docs/
│   ├── 2026-04-27-trammel-design.md               (this spec, moved here)
│   └── 2026-04-27-trammel-implementation.md       (this plan, here)
├── examples/
│   └── trammel.toml                                annotated reference config
└── crates/
    ├── trammel/
    │   ├── Cargo.toml
    │   ├── src/
    │   │   ├── lib.rs                              public API: Config, Violation, run()
    │   │   ├── main.rs                             clap-based CLI
    │   │   ├── error.rs                            anyhow re-export + custom Error enum
    │   │   ├── config/
    │   │   │   ├── mod.rs                          load + validate
    │   │   │   └── schema.rs                       serde types
    │   │   ├── glob/
    │   │   │   ├── mod.rs                          re-exports
    │   │   │   ├── import_path.rs                  import-path glob (* matches across ::)
    │   │   │   ├── ident.rs                        identifier glob (* matches any chars)
    │   │   │   └── fs_path.rs                      thin wrapper over globset
    │   │   ├── layers.rs                           Layer struct, classify(path)
    │   │   ├── visitor.rs                          syn::visit::Visit driver
    │   │   ├── violations.rs                       Violation struct, output formatter
    │   │   └── rules/
    │   │       ├── mod.rs                          dispatch + Rule enum
    │   │       ├── forbidden_imports.rs
    │   │       ├── forbidden_inline_paths.rs
    │   │       ├── forbidden_macros.rs
    │   │       ├── forbidden_methods.rs
    │   │       ├── required_struct_attrs.rs
    │   │       ├── n_plus_one.rs
    │   │       ├── fs_layout.rs                    must_exist + must_not_exist
    │   │       └── file_content_scan.rs
    │   └── tests/
    │       ├── integration.rs                      driver: walks fixtures/ dirs
    │       └── fixtures/
    │           ├── forbidden_imports/{ok,fail}/
    │           ├── forbidden_inline_paths/{ok,fail}/
    │           ├── forbidden_macros/{ok,fail}/
    │           ├── forbidden_methods/{ok,fail}/
    │           ├── required_struct_attrs/{ok,fail}/
    │           ├── n_plus_one/{ok,fail}/
    │           ├── fs_layout/{ok,fail}/
    │           └── file_content_scan/{ok,fail}/
    └── trammel-attrs/
        ├── Cargo.toml                              proc-macro = true
        └── src/lib.rs                              #[allow_n_plus_one]
```

### Repo: `~/src/edms/north-star/` (existing, migration phase)

- Modify: `Cargo.toml` (workspace) — remove arch-lint members, add trammel-attrs dep
- Delete: `crates/arch-lint/`
- Delete: `crates/arch-lint-attrs/`
- Modify: every file currently importing `arch_lint_attrs::allow_n_plus_one` → switch to `trammel_attrs::allow_n_plus_one`
- Create: `trammel.toml` at repo root encoding all 20 arch-lint rule codes
- Modify: `Makefile` — rename `arch-lint:` target to `trammel:`, change command
- Modify: `.git/hooks/pre-push` — switch arch-lint command to `trammel check`
- Modify: `.github/workflows/ci.yml` — install trammel via `cargo install --locked`, replace step

---

## Bootstrap

- [ ] 1. Verify `~/src/rll/` exists; `mkdir -p ~/src/rll/trammel` and `cd` there.

- [ ] 2. `git init`. Create `.gitignore` containing `target/`, `Cargo.lock` is **kept** (this is a binary crate so we want a reproducible lockfile), `*.swp`, `.DS_Store`.

- [ ] 3. Create `LICENSE` containing the verbatim Apache License 2.0 text (download from `https://www.apache.org/licenses/LICENSE-2.0.txt` and check it in).

- [ ] 4. Create `NOTICE`:
   ```
   trammel
   Copyright (c) 2026 Riverline Labs LLC

   This product includes software developed by Riverline Labs LLC.
   Licensed under the Apache License, Version 2.0 (the "License").
   ```

- [ ] 5. Create workspace `Cargo.toml`:
   ```toml
   [workspace]
   resolver = "2"
   members = ["crates/trammel", "crates/trammel-attrs"]

   [workspace.package]
   version = "0.1.0"
   edition = "2021"
   rust-version = "1.83"
   license = "Apache-2.0"
   repository = "https://github.com/riverline-labs/trammel"
   homepage = "https://github.com/riverline-labs/trammel"
   authors = ["Riverline Labs LLC"]

   [workspace.dependencies]
   syn = { version = "2", features = ["full", "visit", "extra-traits"] }
   proc-macro2 = { version = "1", features = ["span-locations"] }
   quote = "1"
   walkdir = "2"
   globset = "0.4"
   serde = { version = "1", features = ["derive"] }
   toml = "0.8"
   anyhow = "1"
   clap = { version = "4", features = ["derive"] }
   ```

- [ ] 6. Create `crates/trammel/Cargo.toml`:
   ```toml
   [package]
   name = "trammel"
   description = "Config-driven architectural conformance tool for Rust workspaces."
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   license.workspace = true
   repository.workspace = true
   homepage.workspace = true
   authors.workspace = true
   readme = "../../README.md"
   keywords = ["lint", "architecture", "conformance"]
   categories = ["development-tools"]

   [lib]
   name = "trammel"
   path = "src/lib.rs"

   [[bin]]
   name = "trammel"
   path = "src/main.rs"

   [dependencies]
   syn.workspace = true
   proc-macro2.workspace = true
   walkdir.workspace = true
   globset.workspace = true
   serde.workspace = true
   toml.workspace = true
   anyhow.workspace = true
   clap.workspace = true
   ```

- [ ] 7. Create `crates/trammel-attrs/Cargo.toml`:
   ```toml
   [package]
   name = "trammel-attrs"
   description = "Marker attributes consumed by trammel. Compile-time no-ops."
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   license.workspace = true
   repository.workspace = true
   homepage.workspace = true
   authors.workspace = true
   readme = "../../README.md"
   keywords = ["lint", "architecture", "trammel"]

   [lib]
   proc-macro = true

   [dependencies]
   proc-macro2.workspace = true
   quote.workspace = true
   syn.workspace = true
   ```

- [ ] 8. Create `crates/trammel-attrs/src/lib.rs` (port from arch-lint-attrs verbatim, with header):
   ```rust
   // Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
   //! Marker attributes consumed by `trammel`. All no-ops at compile time.

   use proc_macro::TokenStream;

   /// Suppresses trammel's `n_plus_one` rule inside the annotated function.
   ///
   /// Use only when the loop genuinely cannot be batched: fan-out writes,
   /// polling/retry loops, stream consumers that process one element at a
   /// time. Every use is a claim that batching is impossible, not merely
   /// inconvenient.
   #[proc_macro_attribute]
   pub fn allow_n_plus_one(_args: TokenStream, item: TokenStream) -> TokenStream {
       item
   }
   ```

- [ ] 9. Create `crates/trammel/src/lib.rs` and `src/main.rs` as stubs so the workspace compiles:
   ```rust
   // src/lib.rs
   // Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
   //! Config-driven architectural conformance tool for Rust workspaces.
   ```
   ```rust
   // src/main.rs
   // Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
   fn main() {
       println!("trammel: stub");
   }
   ```

- [ ] 10. `cargo build` — expect a clean build with both crates.

- [ ] 11. `cargo fmt --all && cargo clippy --all-targets -- -D warnings`. Expect zero warnings.

- [ ] 12. Create `README.md` skeleton with sections: *What it is*, *Install*, *Configure (link to spec)*, *Run*, *License*. Keep it short — fill it out as features land.

- [ ] 13. Create `CHANGELOG.md` with a `## [Unreleased]` heading.

- [ ] 14. Create `.github/workflows/ci.yml`:
   ```yaml
   name: ci
   on:
     push: { branches: [main] }
     pull_request:
   jobs:
     test:
       strategy:
         matrix:
           os: [ubuntu-latest, macos-latest]
       runs-on: ${{ matrix.os }}
       steps:
         - uses: actions/checkout@v4
         - uses: dtolnay/rust-toolchain@stable
         - run: cargo fmt --all -- --check
         - run: cargo clippy --all-targets -- -D warnings
         - run: cargo test --all
   ```

- [ ] 15. `git add -A && git commit -m "chore: bootstrap trammel workspace"`. (Do not push yet — repo doesn't exist on GitHub.)

---

## Engine: Glob primitives (TDD)

- [ ] 16. Create `crates/trammel/src/glob/mod.rs`, `import_path.rs`, `ident.rs`, `fs_path.rs` as empty modules wired up via `pub mod` from `lib.rs`.

- [ ] 17. In `glob/import_path.rs`, write a failing unit test for the import-path matcher:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::matches;
       #[test]
       fn matches_examples_from_spec() {
           assert!(matches("db", "db"));
           assert!(!matches("db", "db::User"));
           assert!(matches("db::*", "db::User"));
           assert!(matches("db::*", "db::queries::find"));
           assert!(matches("axum*", "axum"));
           assert!(matches("axum*", "axum::http"));
           assert!(matches("axum*", "axum::extract::Json"));
           assert!(matches("*transports*", "crate::transports::web"));
           assert!(matches("*transports*", "transports::extractors"));
           assert!(matches("crate::db*", "crate::db"));
           assert!(matches("crate::db*", "crate::db::User"));
           assert!(matches("*::db::*", "crate::db::User"));
           assert!(!matches("axum*", "tower::axum"));
       }
   }
   ```

- [ ] 18. Run `cargo test -p trammel glob::import_path`. Expect compile failure (no `matches` fn yet).

- [ ] 19. Implement `pub fn matches(pattern: &str, path: &str) -> bool` in `glob/import_path.rs` by translating the pattern into a regex anchored at both ends, where `*` becomes `.*` and other regex-meaningful characters are escaped. Use `regex` only if absolutely needed; otherwise a hand-rolled scan: split pattern on `*` into literal chunks, then match chunks left-to-right (first chunk must be a prefix, last must be a suffix, middle chunks must appear in order).

- [ ] 20. `cargo test -p trammel glob::import_path` — expect green.

- [ ] 21. In `glob/ident.rs`, write a failing test:
   ```rust
   #[test]
   fn identifier_globs() {
       assert!(matches("Stub*", "Stub"));
       assert!(matches("Stub*", "StubFoo"));
       assert!(matches("Stub*", "Stubby"));
       assert!(!matches("Stub*", "TStub"));
       assert!(matches("*Port", "TenantPort"));
       assert!(!matches("Stub", "StubFoo"));      // exact match
   }
   ```

- [ ] 22. Implement `glob/ident.rs::matches` (same algorithm as import_path; identifier-class chars are unrestricted because `*` is unrestricted). Treat `**` as a config error: write a `validate(pattern)` that rejects `**`.

- [ ] 23. Test passes. `cargo test -p trammel glob::ident`.

- [ ] 24. In `glob/fs_path.rs`, expose two helpers backed by `globset::Glob`:
   ```rust
   pub fn build_set(globs: &[String]) -> Result<GlobSet, anyhow::Error> { … }
   pub fn matches(set: &GlobSet, rel_path: &str) -> bool { set.is_match(rel_path) }
   ```
   Write a quick smoke test against `app/**`, `transports/web/**`, `transports/web/middleware.rs`.

- [ ] 25. `cargo test -p trammel glob::fs_path`. Green.

- [ ] 26. `cargo fmt && cargo clippy -- -D warnings && git commit -am "feat(glob): import-path, identifier, and fs glob matchers"`.

---

## Engine: Config schema + validation

- [ ] 27. Create `crates/trammel/src/config/schema.rs` and `mod.rs`. Wire `pub mod config` from `lib.rs`.

- [ ] 28. In `schema.rs`, define serde structs that mirror §4 of the spec exactly:
   ```rust
   use serde::Deserialize;
   #[derive(Deserialize, Debug)]
   pub struct Config {
       #[serde(default = "default_src_root")]
       pub src_root: String,
       #[serde(default)]
       pub layers: Vec<Layer>,
       #[serde(default)]
       pub forbidden_imports: Vec<ForbiddenImports>,
       #[serde(default)]
       pub forbidden_inline_paths: Vec<ForbiddenInlinePaths>,
       #[serde(default)]
       pub forbidden_macros: Vec<ForbiddenMacros>,
       #[serde(default)]
       pub forbidden_methods: Vec<ForbiddenMethods>,
       #[serde(default)]
       pub required_struct_attrs: Vec<RequiredStructAttrs>,
       #[serde(default)]
       pub fs_must_exist: Vec<FsMustExist>,
       #[serde(default)]
       pub fs_must_not_exist: Vec<FsMustNotExist>,
       #[serde(default)]
       pub file_content_scan: Vec<FileContentScan>,
       pub n_plus_one: Option<NPlusOne>,
   }
   ```
   And one struct per rule kind matching the TOML field names from the spec. Default `src_root` to `"src"`.

- [ ] 29. Write a failing test that loads `tests/fixtures/config/full.toml` (which you'll create next) and asserts each rule kind has at least one entry parsed correctly.

- [ ] 30. Create `crates/trammel/tests/fixtures/config/full.toml` exactly mirroring north-star's intended config (every rule kind covered with one realistic entry). Use the spec's §4 examples as the starting set.

- [ ] 31. Run the test; iterate until green. Common pitfalls: serde rename for `[[forbidden_inline_paths]]` (default field-name match works since you defined the struct field plural), `Option<String>` vs `String` for optional fields, `position = "any"` defaulted by `#[serde(default = "default_position")]`.

- [ ] 32. In `config/mod.rs`, add `pub fn load(path: &Path) -> Result<Config>` that reads + parses TOML.

- [ ] 33. Add `pub fn validate(cfg: &Config) -> Result<()>` that walks the parsed config and rejects:
   - any rule entry with neither `in_layers` nor `in_files` (other than `fs_*` and `file_content_scan`)
   - any layer name referenced in a rule's `in_layers` that doesn't exist in `[[layers]]`
   - identifier patterns containing `**`
   - `n_plus_one.layer_assumes_query` containing a layer not in `n_plus_one.in_layers`

- [ ] 34. Write tests covering each validation rejection with a minimal failing TOML fixture per case.

- [ ] 35. `cargo fmt && cargo clippy && cargo test -p trammel config && git commit -am "feat(config): schema + validation"`.

---

## Engine: Layer classification

- [ ] 36. Create `crates/trammel/src/layers.rs`. Wire `pub mod layers`.

- [ ] 37. Define `pub struct LayerSet { /* compiled glob sets per layer */ }` and `pub fn build(cfg: &Config) -> Result<LayerSet>`.

- [ ] 38. Define `pub fn classify<'a>(set: &'a LayerSet, rel_path: &str) -> Option<&'a Layer>`. First-match semantics, in declaration order. `None` for paths matching no layer.

- [ ] 39. Define `pub fn is_exempt(set: &LayerSet, layer: &Layer, rel_path: &str) -> bool` for per-layer exempt_files.

- [ ] 40. Write tests: classification of `app/foo.rs`, `system/connectors/db.rs` (system, exempt), `transports/web/middleware.rs` (transports_web, exempt), `tests/foo.rs` (tests, implicit_test_context), `something/random.rs` (None).

- [ ] 41. Green tests. Commit `feat(layers): classification + exemptions`.

---

## Engine: Violation type + output

- [ ] 42. Create `crates/trammel/src/violations.rs`:
   ```rust
   #[derive(Debug, Clone)]
   pub struct Violation {
       pub file: PathBuf,
       pub line: usize,
       pub rule: String,
       pub message: String,
   }
   ```

- [ ] 43. Add `pub fn render(violations: &[Violation]) -> String` matching arch-lint's grouped-by-rule output format:
   ```
   ── RULE_NAME (N violations) ──
     path:line — message
     …

   trammel: FAILED — N violations found.
   ```
   Empty input renders `trammel: OK — no violations found.`.

- [ ] 44. Snapshot-test the renderer against a small Vec of fixture violations. Use `expect_test` or just hand-coded `assert_eq!` against a multiline `&str`.

- [ ] 45. Commit `feat(violations): grouped output formatter`.

---

## Engine: Visitor scaffold

- [ ] 46. Create `crates/trammel/src/visitor.rs`. The driver does:
   ```rust
   pub fn check_file(
       cfg: &Config,
       layer_set: &LayerSet,
       path: &Path,
       rel_path: &str,
       ast: &syn::File,
       violations: &mut Vec<Violation>,
   ) { … }
   ```

- [ ] 47. Inside, define `struct Visitor<'a> { cfg, layer_set, layer, in_test_context, loop_depth, n_plus_one_suppressed, path, violations }`. `impl<'ast, 'a> syn::visit::Visit<'ast> for Visitor<'a> { … }`.

- [ ] 48. Stub all `visit_*` methods to delegate up to `syn::visit::visit_*` (so traversal continues). The actual rule logic gets wired in subsequent tasks.

- [ ] 49. Initialize `in_test_context = layer.implicit_test_context.unwrap_or(false)`.

- [ ] 50. Implement test-context propagation in `visit_item_fn`, `visit_item_impl`, `visit_item_mod`: detect `#[test]`, `#[cfg(test)]`, `#[cfg(any(test, ...))]` attributes (token-tree string contains `"test"` and ident is `cfg` or `test`), save/restore the flag.

- [ ] 51. In `visit_item_fn`: also save/restore `loop_depth = 0` (matches arch-lint main.rs:888-914).

- [ ] 52. In `visit_item_fn`: detect the n_plus_one opt-out attribute by matching the configured `opt_out_attribute` against the **last** segment of every attribute path; save/restore `n_plus_one_suppressed`.

- [ ] 53. Implement `loop_depth` tracking: increment in `visit_expr_for_loop`, `visit_expr_while`, `visit_expr_loop`, and any `visit_expr_method_call` whose method name is in `cfg.n_plus_one.combinators`. Decrement on exit.

- [ ] 54. Write a unit test using `syn::parse_str` on a small snippet that has nested fn / loops, and assert via instrumentation that `loop_depth` and `in_test_context` are tracked correctly. (Use a minimal `RecordingVisitor` test harness in `visitor.rs#cfg(test)`.)

- [ ] 55. Commit `feat(visitor): scaffolding + test/loop/opt-out propagation`.

---

## Rule: forbidden_imports

- [ ] 56. Create `crates/trammel/src/rules/mod.rs` and `forbidden_imports.rs`. Add `pub mod rules` to `lib.rs`.

- [ ] 57. Add fixture dir `tests/fixtures/forbidden_imports/`:
   - `ok/app_clean.rs` — uses `crate::system`, `std`, `serde` (allowed)
   - `fail/app_uses_axum.rs` — `use axum::http::StatusCode;`
   - `fail/app_uses_transports.rs` — `use crate::transports::web::router;`
   - `trammel.toml` per fixture set

- [ ] 58. In `forbidden_imports.rs`, add `pub fn check(visitor: &mut Visitor, node: &syn::ItemUse)`. For each `[[forbidden_imports]]` entry whose `in_layers`/`in_files` matches the current file: stringify the use tree, check against `patterns` (import-path glob from §4.4.1).

- [ ] 59. Helper: port `use_tree_to_string` from arch-lint main.rs:997. Cover `Path`, `Name`, `Rename`, `Glob`, `Group` (group becomes comma-joined, but for matching, expand each item).

- [ ] 60. Wire dispatcher: `Visit::visit_item_use` calls `forbidden_imports::check`, then continues traversal.

- [ ] 61. Write integration test that runs `trammel check` on each fixture; OK fixtures produce zero violations, FAIL fixtures produce exactly the expected rules.

- [ ] 62. Commit `feat(rules): forbidden_imports`.

---

## Rule: forbidden_inline_paths

- [ ] 63. Add fixture dir `tests/fixtures/forbidden_inline_paths/`:
   - `ok/app_uses_db_through_app.rs`
   - `fail/transports_uses_crate_db.rs` — `let u = crate::db::User { … };`
   - `fail/transports_typeposition_db.rs` — `fn x(u: db::User)`
   - `fail/router_references_app.rs` — `let h = app::deals::list;` (in `transports/web/router.rs`)
   - `ok/test_context_exempt.rs` — same violation but inside `#[test]`
   - `trammel.toml` per set

- [ ] 64. Port `path_to_string` from arch-lint main.rs:1014 into a helper module (use it from any rule that needs it).

- [ ] 65. In `forbidden_inline_paths.rs`, implement two entry points:
   ```rust
   pub fn check_expr(v: &mut Visitor, node: &syn::ExprPath);
   pub fn check_type(v: &mut Visitor, node: &syn::TypePath);
   ```
   Each iterates `[[forbidden_inline_paths]]` entries whose scope (`in_layers`/`in_files`) matches the current file AND whose `position` admits the call-site (`expr` / `type` / `any`). Honor `allow_in_test_context` against `visitor.in_test_context`.

- [ ] 66. Wire into visitor's `visit_expr_path` / `visit_type_path`.

- [ ] 67. Run the fixture tests; verify the test-context fixture produces zero violations and the others produce the expected rule names.

- [ ] 68. Commit `feat(rules): forbidden_inline_paths with position`.

---

## Rule: forbidden_macros

- [ ] 69. Fixtures `tests/fixtures/forbidden_macros/`:
   - `fail/app_uses_sqlx_query.rs` — `sqlx::query!("SELECT 1")`
   - `fail/transports_uses_bare_query.rs` — `query!("SELECT 1")` (only flagged if `bare_names_in_layers` includes transports — test the inverse case in `ok/`)
   - `ok/db_uses_sqlx_query.rs` — db layer is fine

- [ ] 70. In `forbidden_macros.rs`, implement `pub fn check(v: &mut Visitor, node: &syn::Macro)`:
   - Compute full path string (joined `::`) and final segment.
   - For each `[[forbidden_macros]]` entry whose layer matches: check `qualified_names` against full path (import-path glob), then check `bare_names` exact-match against final segment but only if current layer ∈ `bare_names_in_layers`.

- [ ] 71. Wire into `visit_macro`.

- [ ] 72. Tests green. Commit `feat(rules): forbidden_macros`.

---

## Rule: forbidden_methods

- [ ] 73. Fixtures: `fail/app_uses_unwrap.rs` (`.unwrap()` outside test), `ok/test_uses_unwrap.rs` (inside `#[test]`).

- [ ] 74. In `forbidden_methods.rs`, implement `pub fn check(v: &mut Visitor, node: &syn::ExprMethodCall)`. For each entry whose layer matches: if method name ∈ `methods` AND not (allow_in_test_context && in_test_context), record a violation at `node.method.span().start().line`.

- [ ] 75. Wire into `visit_expr_method_call` (note: this same method is where loop_depth tracking happens for combinators — keep the two concerns separate within the visitor).

- [ ] 76. Tests green. Commit `feat(rules): forbidden_methods`.

---

## Rule: required_struct_attrs

- [ ] 77. Fixtures:
   - `fail/system_ungated_stub.rs` — `pub struct StubFoo;` (no `#[cfg(test)]`)
   - `ok/system_gated_stub.rs` — `#[cfg(any(test, feature = "testing"))] pub struct StubFoo;`
   - `fail/system_ungated_impl.rs` — `impl StubFoo { fn x() {} }` (also_apply_to_impls)

- [ ] 78. In `required_struct_attrs.rs`, two entry points:
   ```rust
   pub fn check_struct(v: &mut Visitor, node: &syn::ItemStruct);
   pub fn check_impl(v: &mut Visitor, node: &syn::ItemImpl);
   ```

- [ ] 79. For struct: match `struct_name_pattern` against `node.ident`. If matched, check `required_any_of` against the union of attribute token-tree strings (substring match per §4.3.5 spec); if no match, record violation.

- [ ] 80. For impl (when `also_apply_to_impls = true`): stringify `node.self_ty` via `quote!(#self_ty).to_string()`, then trim/collapse whitespace, then match the pattern against the resulting string.

- [ ] 81. Wire into `visit_item_struct` and `visit_item_impl`.

- [ ] 82. Tests green. Commit `feat(rules): required_struct_attrs`.

---

## Rule: n_plus_one

- [ ] 83. Fixtures:
   - `fail/app_loop_await.rs`:
     ```rust
     async fn fanout(ids: &[Uuid]) -> anyhow::Result<()> {
         for id in ids {
             let _ = db::user::get(id).await?;
         }
         Ok(())
     }
     ```
   - `ok/app_allowed.rs`: same shape but `#[trammel_attrs::allow_n_plus_one]` on the fn.
   - `ok/app_test.rs`: same shape inside `#[tokio::test]`.
   - `fail/app_combinator_then.rs`: `.then(|id| async move { db::user::get(id).await })`.
   - `fail/db_loop_await.rs`: `db/` layer; layer_assumes_query bypasses path scan.
   - `ok/app_loop_no_db.rs`: loop-await on a non-db expression (channel recv).

- [ ] 84. In `n_plus_one.rs`, implement `pub fn check_await(v: &mut Visitor, node: &syn::ExprAwait)`. Honors:
   - `loop_depth > 0`
   - layer ∈ `cfg.n_plus_one.in_layers`
   - `!v.in_test_context`
   - `!v.n_plus_one_suppressed`

   If layer ∈ `layer_assumes_query`, record violation. Else call `expr_hits_db(&node.base)`.

- [ ] 85. Port `expr_hits_db` from arch-lint main.rs:314 with these changes:
   - Use configured `db_path_patterns` (import-path glob) instead of hardcoded `db::` / `sqlx::`.
   - Use configured `db_macros` (exact-match on final segment) instead of hardcoded macro names.
   - Preserve the recursion guard: do not visit nested `ItemFn` or `ItemImpl` bodies.

- [ ] 86. Wire into `visit_expr_await`.

- [ ] 87. Run all fixtures. Manually inspect output to confirm exactly the expected lines flag.

- [ ] 88. Commit `feat(rules): n_plus_one`.

---

## Rule: fs_layout

- [ ] 89. Fixtures: `tests/fixtures/fs_layout/{ok,fail}/` mocked as directory trees. Run pre-AST: do not require `.rs` parsing.

- [ ] 90. In `fs_layout.rs`, implement `pub fn check(cfg: &Config, src_root: &Path, project_root: &Path, violations: &mut Vec<Violation>)`. Iterate `fs_must_exist` and `fs_must_not_exist` entries, check the filesystem, record violations with `line: 0`.

- [ ] 91. Note: `fs_*` paths are relative to project root, not src_root (e.g., `src/lib.rs`, `src/adapters`). Document this explicitly in `config/schema.rs` doc-comments.

- [ ] 92. Tests + commit `feat(rules): fs_layout`.

---

## Rule: file_content_scan

- [ ] 93. Fixtures:
   - `fail/template_persona.html` (under `transports/web/templates/`) containing `session.persona`
   - `ok/template_clean.html`
   - `fail/transport_is_admin.rs` containing `.is_admin()`

- [ ] 94. In `file_content_scan.rs`, implement `pub fn check(cfg: &Config, project_root: &Path, violations: &mut Vec<Violation>)`. For each entry: build glob set from `glob`/`exclude_glob`, walk matching files, scan contents for any `forbidden_substrings`. Report `line: 0` (substring offsets are not line-mapped in v0.1).

- [ ] 95. Tests + commit `feat(rules): file_content_scan`.

---

## Engine: Top-level run()

- [ ] 96. In `lib.rs`, implement `pub fn run(cfg: Config, project_root: &Path) -> Result<Vec<Violation>>`:
   1. Validate config (`config::validate`).
   2. Build `LayerSet`.
   3. Run `fs_layout::check` and `file_content_scan::check`.
   4. Walk `project_root.join(&cfg.src_root)` with `walkdir` filtering `.rs` files; for each: parse with `syn::parse_file`, classify layer, build `Visitor`, run `check_file`. Skip parse errors with an `eprintln!` warning (matches arch-lint behavior).
   5. Return aggregated violations.

- [ ] 97. Add an end-to-end integration test: write a small project tree under `tests/fixtures/full_project/{ok,fail}/`, run `trammel::run`, assert violation counts.

- [ ] 98. Commit `feat: top-level run()`.

---

## CLI

- [ ] 99. Replace stub `main.rs` with clap-derive structure:
   ```rust
   #[derive(clap::Parser)]
   struct Cli {
       #[command(subcommand)]
       command: Option<Cmd>,
   }
   #[derive(clap::Subcommand)]
   enum Cmd {
       Check(CheckArgs),
       Rules(RulesCmd),
   }
   ```
   Default subcommand is `Check` if none given.

- [ ] 100. `CheckArgs` carries `--config <PATH>` (default `./trammel.toml`) and `--src <PATH>` (override `cfg.src_root`).

- [ ] 101. `Rules` has nested subcommands `List` (default) and `Explain { rule: String }`.

- [ ] 102. Wire `check` to `trammel::run` + `violations::render`. Exit codes: 0 clean, 1 violations, 2 config/IO error.

- [ ] 103. `rules list` prints every active rule code from the loaded config (one per line, with the rule kind in parentheses, e.g. `APP_NO_AXUM (forbidden_imports)`).

- [ ] 104. `rules explain RULE_NAME` finds the entry by `rule` field, prints the rule kind, the configured message, and the source location (`trammel.toml line ?`). For v0.1 source-line reporting can be best-effort or omitted; prefer to drop it rather than ship a half-baked locator.

- [ ] 105. Smoke-test by hand against `tests/fixtures/full_project/fail/`. Expect grouped output and exit 1.

- [ ] 106. Commit `feat(cli): check + rules subcommands`.

---

## Examples + README

- [ ] 107. Create `examples/trammel.toml` annotated with comments; this is the future north-star config and serves as the user-facing reference.

- [ ] 108. Flesh out `README.md`:
   - One-paragraph What
   - Install: `cargo install --locked trammel` and `trammel-attrs = "0.1"` for the attribute
   - Configure: link to `docs/2026-04-27-trammel-design.md` for full schema; show a 30-line minimum config inline
   - Run: `trammel check` from project root
   - Wire as a pre-push hook (snippet)
   - License: Apache 2.0

- [ ] 109. Update `CHANGELOG.md` with `## [0.1.0] – TBD` and bullet points for what shipped.

- [ ] 110. `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --all`. Expect green across the board.

- [ ] 111. Commit `docs: README, CHANGELOG, example config`.

---

## Repo publish

- [ ] 112. Confirm with user before any irreversible action: ready to create the public GitHub repo and publish to crates.io? (Both are public-visible and not easily undone.)

- [ ] 113. Create the GitHub repo at `riverline-labs/trammel` (public, Apache 2.0 license, no boilerplate readme — we already have one). User performs this step in the browser; assistant should not create repos in third-party orgs without explicit go-ahead.

- [ ] 114. `git remote add origin git@github.com:riverline-labs/trammel.git && git push -u origin main`.

- [ ] 115. Verify CI green on the first push (the matrix workflow runs).

- [ ] 116. Tag `v0.1.0`: `git tag -a v0.1.0 -m "v0.1.0" && git push origin v0.1.0`.

- [ ] 117. `cargo publish -p trammel-attrs` (publish the proc-macro crate first, since `trammel` does not depend on it but consumers do; ordering doesn't matter strictly but proc-macro first is conventional).

- [ ] 118. `cargo publish -p trammel`.

- [ ] 119. Verify both crates appear on crates.io and `cargo install --locked trammel` works from a fresh shell.

- [ ] 120. Commit nothing further in this repo for the migration phase; switch to north-star.

---

## North-star migration

> Performed in `~/src/edms/north-star/` after trammel v0.1.0 is published.

- [ ] 121. From north-star, `cargo install --locked trammel` to put the binary on PATH.

- [ ] 122. Add `trammel-attrs = "0.1"` to north-star's workspace `Cargo.toml` `[workspace.dependencies]` table; remove `arch-lint-attrs` workspace member entry.

- [ ] 123. Find and replace every import: `grep -rln 'arch_lint_attrs::allow_n_plus_one' src/ crates/`. For each match: `use arch_lint_attrs::allow_n_plus_one;` → `use trammel_attrs::allow_n_plus_one;`. Same for any `#[arch_lint_attrs::allow_n_plus_one]` qualified attribute references → `#[trammel_attrs::allow_n_plus_one]`.

- [ ] 124. `cargo build` — expect green; commit `chore(deps): switch from arch-lint-attrs to trammel-attrs`.

- [ ] 125. Delete `crates/arch-lint-attrs/` directory entirely. Remove its workspace member entry from `Cargo.toml`. `cargo build` — green. Commit `chore: drop arch-lint-attrs crate`.

- [ ] 126. Create `trammel.toml` at north-star repo root. Source it from the spec's §4 examples (which were authored from arch-lint's behavior) and from `~/src/rll/trammel/examples/trammel.toml`. Encode all 20 arch-lint rule codes per the §5 mapping.

- [ ] 127. Run `trammel check`. Expect zero violations against current main.

- [ ] 128. If non-zero violations: each one is either (a) a config bug (fix `trammel.toml`) or (b) a real arch-lint regression that arch-lint also flagged in CI. Verify by running `cargo run -p arch-lint` against the same tree and comparing rule codes. Iterate until both produce zero violations.

- [ ] 129. Update `Makefile`:
   ```
   .PHONY: ... trammel ...     # rename arch-lint
   trammel: ## Architecture conformance — hard push gate
   	trammel check
   verify: fmt-check trammel build clippy test
   ```
   Remove the `arch-lint:` target.

- [ ] 130. Update `.git/hooks/pre-push`: replace `cargo run -p arch-lint -- --src src` with `trammel check`.

- [ ] 131. Update `.github/workflows/ci.yml`: replace the arch-lint step with two steps:
   ```yaml
   - name: install trammel
     run: cargo install --locked trammel --version "^0.1"
   - name: trammel check
     run: trammel check
   ```
   Cache the cargo bin directory across runs to avoid 60-90s install on every CI job.

- [ ] 132. `make verify` — expect green. Commit `chore(ci,git): switch arch-lint gate to trammel`.

- [ ] 133. Delete `crates/arch-lint/` directory entirely. Remove its workspace member entry. Remove `arch-lint:` references from `Cargo.toml`, `Makefile`, docs, and any CLAUDE.md mentions.

- [ ] 134. `cargo build && cargo clippy --all-targets -- -D warnings && cargo test --all && trammel check`. All green.

- [ ] 135. Commit `chore: drop arch-lint crate; trammel is the new gate`. Push.

- [ ] 136. Verify CI green on the merged migration PR.

---

## Done check

- [ ] 137. `trammel check` passes against north-star main with the same coverage as arch-lint did.
- [ ] 138. `crates.io` shows `trammel` v0.1.0 and `trammel-attrs` v0.1.0, both Apache 2.0, owner Riverline Labs.
- [ ] 139. `riverline-labs/trammel` repo public, CI green on `main`, README readable, examples present.
- [ ] 140. North-star tree contains zero `arch_lint*` identifiers (`grep -rn 'arch_lint\|arch-lint' .` returns only stale references in old commit messages or untouched docs).
