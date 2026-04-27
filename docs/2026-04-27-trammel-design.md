# trammel — Architectural Conformance Tool (Design)

**Status:** Draft
**Date:** 2026-04-27
**Owner:** Riverline Labs LLC
**Repo target:** `github.com/riverline-labs/trammel` (public, Apache 2.0)
**Local path:** `~/src/rll/trammel`

## 1. Problem

`crates/arch-lint/` (1,020 lines) and `crates/arch-lint-attrs/` (20 lines) in north-star are an in-tree, project-specific architectural conformance tool. They walk Rust source with `syn`, classify files by directory into "layers" (`app/`, `system/`, `db/`, `transports/web/`, `transports/cli/`, `lib/`, `tests/`), and enforce rules: banned imports per layer, banned macro invocations per layer, banned inline path references, banned method calls (`.unwrap()`, `.expect()`), required cfg-gating on stub structs, filesystem layout assertions (e.g., `src/lib.rs` must exist; `src/adapters/` must not), file-content scans on templates, and N+1 detection (loop or iterator-combinator + `.await` on a path that touches `db::` or `sqlx::`).

The tool is gated pre-push and in CI. It works well in practice but:

1. It is **not reusable**. Every rule's parameters (paths, banned identifiers, layer names) are baked into Rust constants. A second project would have to fork.
2. It mixes **rule engine** (AST visitors, layer classification, attribute introspection) with **project-specific configuration** (`AnyUser`/`AnyDeal` identifier list, `transports/web/templates/**` glob, the specific paths that must not exist). The engine is general; the configuration is north-star's.
3. It is **owned by north-star** but is generic IP. Riverline Labs would prefer to publish it as a standalone open-source tool.

This spec defines `trammel`: a config-driven extraction of arch-lint into its own Apache 2.0 repo, owned by Riverline Labs LLC, that north-star will consume as an external dependency.

## 2. Scope

### 2.1 In scope (v0.1.0)

- Extract arch-lint's rule engine into a standalone Rust crate.
- Define a TOML configuration schema (`trammel.toml`) expressive enough to encode every rule currently in arch-lint.
- Ship a `trammel` CLI binary and a `trammel-attrs` proc-macro crate (for the `#[allow_n_plus_one]` opt-out attribute).
- Publish both crates to `crates.io` under Apache 2.0 with copyright held by Riverline Labs LLC.
- Migrate north-star to consume `trammel` and delete `crates/arch-lint*`.

### 2.2 Not in scope (v0.1.0)

- Editor / LSP integration. Pre-push gate is the integration point.
- A library API for user-defined rules. The set of rule **kinds** is fixed and built into the binary; user-defined Rust rules are deferred until a real second consumer asks. (Configuration is fully customizable; the kinds are not.)
- Severity levels. Every violation is an error.
- Line- or block-level inline suppression comments (`# trammel-allow: RULE`). Suppression is via the typed attribute (`#[allow_n_plus_one]`) or by restructuring the code.
- Multi-file or layered configuration. One `trammel.toml` per project.
- Migrating any of arch-lint's project-specific rules into trammel as built-ins; **all** project-specific knowledge moves into north-star's `trammel.toml`.

## 3. Project Layout

```
~/src/rll/trammel/                       →  github.com/riverline-labs/trammel
├── Cargo.toml                            (workspace)
├── LICENSE                               Apache 2.0
├── NOTICE                                Copyright (c) 2026 Riverline Labs LLC
├── README.md                             usage, config schema, rule kinds reference
├── CHANGELOG.md
├── examples/
│   └── trammel.toml                      annotated reference config
├── crates/
│   ├── trammel/                          rule engine + CLI (lib + bin in one crate)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs                    pub Config, run(), Violation
│   │   │   ├── main.rs                   thin CLI dispatch
│   │   │   ├── config/
│   │   │   │   ├── mod.rs
│   │   │   │   └── schema.rs             serde types for trammel.toml
│   │   │   ├── layers.rs                 path → layer classification, exempt files
│   │   │   ├── glob.rs                   path/identifier glob matcher
│   │   │   ├── visitor.rs                main syn::visit::Visit driver
│   │   │   └── rules/
│   │   │       ├── mod.rs
│   │   │       ├── forbidden_imports.rs
│   │   │       ├── forbidden_inline_paths.rs
│   │   │       ├── forbidden_macros.rs
│   │   │       ├── forbidden_methods.rs
│   │   │       ├── required_struct_attrs.rs
│   │   │       ├── fs_layout.rs          must_exist + must_not_exist
│   │   │       ├── file_content_scan.rs
│   │   │       └── n_plus_one.rs
│   │   └── tests/
│   │       └── fixtures/                 one passing + one failing fixture per rule kind
│   └── trammel-attrs/                    proc-macro crate
│       ├── Cargo.toml
│       └── src/lib.rs                    #[allow_n_plus_one] (no-op marker)
└── .github/
    └── workflows/
        └── ci.yml                        fmt + clippy + test, on PR and push to main
```

**Why two crates and not three:** `trammel-attrs` is forced — proc-macro crates can only export proc macros, so the attribute cannot live in the main crate. `trammel`'s library and binary collapse into one crate (`[lib]` + `[[bin]]`) until a real second consumer needs the engine standalone; this is YAGNI on extraction, not on capability.

## 4. Configuration Schema

Single file, `trammel.toml`, at the project root by default. Override with `--config`.

### 4.1 Top-level

```toml
src_root = "src"                          # default; relative to CWD
```

### 4.2 Layers

A **layer** is a named set of paths under `src_root`, plus optional per-layer flags. Path classification is first-match, in declaration order. A file matching no layer is classified as `Other` and is skipped by per-layer rules.

```toml
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "system"
paths = ["system/**"]
exempt_files = ["system/connectors/db.rs"]   # excluded from this layer's rules

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]
exempt_files = ["transports/web/middleware.rs"]

[[layers]]
name = "tests"
paths = ["tests.rs", "tests/**"]
implicit_test_context = true              # treat all code in this layer as cfg(test)
```

`exempt_files` is per-layer and lists files where the layer's rules are skipped (used today for `transports/web/middleware.rs` carrying `sqlx`, and `system/connectors/db.rs` legitimately importing `sqlx` to talk to *external* databases).

### 4.3 Rule kinds

Eight kinds. Every rule entry has a required `rule` field (the rule code, e.g., `"APP_NO_AXUM"`) and a `message` (or `message_template`) field. Where applicable, rules accept `allow_in_test_context = true|false` (default `false`).

**Test-context propagation.** A file's effective test-context is the union of:
1. The file's layer has `implicit_test_context = true`.
2. The current AST item is inside a `#[test]` `fn`, a `#[cfg(test)]` `fn` / `impl` / `mod`, or a `#[cfg(any(test, ...))]` `fn` / `impl` / `mod`.

When in test-context, rules with `allow_in_test_context = true` skip the violation. Rules without that field (e.g., `n_plus_one`) define their own test-exemption behavior in their section.

**Scope requirement.** Every rule entry that uses `in_layers`/`in_files` scoping (i.e., everything except `fs_must_exist`, `fs_must_not_exist`, and `file_content_scan` which scope by path glob) must declare at least one of `in_layers` or `in_files`. Omitting both is a config error.

#### 4.3.1 `forbidden_imports`

Flags `use` statements matching configured patterns in configured layers.

```toml
[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*", "*transports*", "sqlx*"]
rule = "APP_BOUNDARY"
message = "app/ must not import {pattern}"
```

`patterns` are import-path globs (`*` = one segment, `**` = any). The `{pattern}` token in the message is substituted with the offending pattern.

#### 4.3.2 `forbidden_inline_paths`

Flags fully-qualified paths in expression or type position (e.g., `crate::db::Foo` in a function body or `db::User` as a type annotation).

```toml
[[forbidden_inline_paths]]
in_layers = ["transports_web", "transports_cli"]
patterns = ["db", "db::*", "crate::db*", "sqlx", "sqlx::*"]
position = "any"                                # default; matches both expr and type position
rule = "TRANSPORTS_NO_DB"
allow_in_test_context = true

[[forbidden_inline_paths]]
in_files = ["src/transports/web/router.rs"]
patterns = ["app::*"]
position = "expr"                               # only flag in expression position
rule = "ROUTER_NO_APP"
```

`in_files` is an alternative scope to `in_layers`, for rules that apply to specific files. A rule entry must have at least one of `in_layers` or `in_files`; omitting both is a config error.

`position` is one of `expr` (expression-position paths only, like `let h = app::foo;`), `type` (type-position paths only, like `fn x(_: db::User)`), or `any` (default, both).

#### 4.3.3 `forbidden_macros`

Flags macro invocations (e.g., `sqlx::query!(...)`).

```toml
[[forbidden_macros]]
in_layers = ["app", "system", "transports_web", "transports_cli"]
qualified_names = ["sqlx::*"]                 # any sqlx-prefixed macro
bare_names = ["query", "query_as", "query_scalar", "query_unchecked"]
bare_names_in_layers = ["app", "system"]      # bare names only flagged in these layers
rule = "NO_SQL_OUTSIDE_DB"
allow_in_test_context = true
```

`qualified_names` accepts the import-path glob syntax from §4.4.1; `sqlx::*` matches `sqlx::query`, `sqlx::query_as`, `sqlx::query_unchecked_with`, etc. The full macro path (joined segments) is matched.

`bare_names` matches the macro's final path segment by exact name (no globs). `bare_names_in_layers` exists because `query!(...)` (unqualified) might come from any in-scope import; in `transports/`, where prelude churn is high, requiring the `sqlx::` prefix is the conservative choice.

#### 4.3.4 `forbidden_methods`

Flags `.method()` calls.

```toml
[[forbidden_methods]]
in_layers = ["app", "system", "db", "transports_web", "transports_cli"]
methods = ["unwrap", "expect"]
allow_in_test_context = true
rule = "NO_UNWRAP_IN_PRODUCTION"
```

#### 4.3.5 `required_struct_attrs`

Flags structs (and optionally their `impl` blocks) matching a name pattern that lack a required attribute.

```toml
[[required_struct_attrs]]
in_layers = ["system"]
struct_name_pattern = "Stub*"             # glob on the struct identifier
required_any_of = [
  "cfg(test)",
  'cfg(any(test, feature = "testing"))',
]
also_apply_to_impls = true
rule = "STUBS_MUST_BE_GATED"
```

**Struct match:** `struct_name_pattern` is matched against the struct's identifier (§4.4.3 identifier glob).

**Impl match (when `also_apply_to_impls = true`):** the pattern is matched against the stringified `self_ty` of the `impl` block. `impl Foo for StubBar` has `self_ty = "StubBar"`; `impl StubBaz` has `self_ty = "StubBaz"`. The trait path (the `Foo` in `impl Foo for X`) is not consulted.

**Attribute match:** an item satisfies `required_any_of` if any of its attributes' stringified token trees (after whitespace normalization) **contain** any of the required-form substrings. This deliberately preserves arch-lint's loose matching: `#[cfg(any(test, feature = "testing"))]` satisfies `cfg(test)` because the token-tree string contains the substring `test`. This loose match also accepts `cfg(test_helper)` and similar variants — that is intentional pragma compatibility, not strictness.

#### 4.3.6 `fs_must_exist` / `fs_must_not_exist`

Filesystem layout assertions, evaluated once per run.

```toml
[[fs_must_exist]]
path = "src/lib.rs"
rule = "LIB_IS_FILE"
message = "library root must be src/lib.rs"

[[fs_must_not_exist]]
paths = [
  "src/lib",
  "src/app_state.rs",
  "src/adapters",
  "src/transports/extractors",
]
rule = "FORBIDDEN_PATHS"
```

`fs_must_not_exist` accepts either a single `path` or a list `paths`. `fs_must_exist` accepts only a single `path` per entry; declare additional required paths with additional `[[fs_must_exist]]` blocks.

#### 4.3.7 `file_content_scan`

Substring scan of file contents under a glob.

```toml
[[file_content_scan]]
glob = "src/transports/web/templates/**"
forbidden_substrings = ["session.persona"]
rule = "TEMPLATE_NO_PERSONA_CHECK"

[[file_content_scan]]
glob = "src/transports/**/*.rs"
exclude_glob = "src/transports/web/templates/**"
forbidden_substrings = [".is_admin()"]
rule = "TRANSPORT_NO_IS_ADMIN"
```

This is intentionally a substring check, not an AST check — it's the right tool for non-Rust files (templates) and for catching rendered-method calls regardless of qualification. Line numbers are reported as 0 (file-level) since substring offsets don't roundtrip cleanly to lines without re-scanning.

#### 4.3.8 `n_plus_one`

Loop or iterator-combinator + `.await` on a configured DB-touching expression.

```toml
[n_plus_one]
in_layers = ["app", "db"]
db_path_patterns = ["db", "db::*", "crate::db*", "sqlx", "sqlx::*"]
db_macros = ["query", "query_as", "query_scalar", "query_unchecked"]
combinators = [
  "map", "for_each", "for_each_concurrent", "then", "and_then",
  "filter_map", "flat_map", "fold",
  "try_for_each", "try_for_each_concurrent", "try_fold",
  "inspect",
]
opt_out_attribute = "allow_n_plus_one"
layer_assumes_query = ["db"]              # in db/, every await is a query — skip the path scan
rule = "N_PLUS_ONE"
```

**Detection:**
- Track `loop_depth`: incremented on `for`, `while`, `loop`, and any method call whose method name is in `combinators`; decremented on exit.
- On every `.await` where `loop_depth > 0` and the file's layer is in `in_layers`, evaluate the awaited expression:
    - If the layer is in `layer_assumes_query`, treat as a violation unconditionally (in `db/`, every await is a query — the whole layer's job).
    - Otherwise, scan the awaited expression's subtree for any path matching `db_path_patterns` (per §4.4.1 import-path globs) **or** any macro invocation whose final path segment matches a `db_macros` entry by exact name. The subtree scan **does not recurse** into nested `fn` or `impl` items: their bodies are not executed by the outer `.await`.
- `loop_depth` is **reset to 0 and saved/restored** on entry to a nested `fn` item (a closure or `async fn` defined inside the loop body executes its body separately, not on each iteration of the outer loop). The reset is **not** applied on `impl` items because `impl` blocks contain item declarations, not executable scope.
- A layer entry that appears in both `in_layers` and `layer_assumes_query` is allowed and means "always-violation" semantics for that layer; a layer in only `in_layers` requires the path/macro scan; a layer in only `layer_assumes_query` is invalid (config error).

**Opt-out:** The `opt_out_attribute` is detected by matching the **final segment** of any attribute on the enclosing `fn` item against the configured name. Both `#[allow_n_plus_one]` and `#[trammel_attrs::allow_n_plus_one]` therefore satisfy `opt_out_attribute = "allow_n_plus_one"`.

**Test exemption:** test code (per layer's `implicit_test_context = true`, or per-fn `#[cfg(test)]` / `#[test]`, or any enclosing `#[cfg(test)]` `mod`/`impl`) is exempt unconditionally — irrespective of `allow_in_test_context` (this rule has no such field; testing context is always exempt).

### 4.4 Glob semantics

Three distinct match domains, each with their own rules.

#### 4.4.1 Import-path patterns (`forbidden_imports.patterns`, `forbidden_inline_paths.patterns`, `n_plus_one.db_path_patterns`, `forbidden_macros.qualified_names`)

`*` matches any sequence of characters, **including `::` separators**. A bare pattern with no `*` is an exact match. The `*` may appear at the start (suffix-match), end (prefix-match), middle, or both ends (substring-match).

| Pattern | Matches |
|---|---|
| `db` | exactly `db` |
| `db::*` | `db::User`, `db::queries::find` |
| `db**` | unused — `**` is treated as `*` (no segment-aware semantics) |
| `axum*` | `axum`, `axum::http`, `axum::extract::Json` |
| `*transports*` | `crate::transports::web`, `transports::extractors`, anything containing `transports` |
| `crate::db*` | `crate::db`, `crate::db::User` |
| `*::db::*` | `crate::db::User`, `super::db::queries` |

This intentionally mirrors arch-lint's existing mix of `starts_with`, `contains`, and `==` checks under one shorthand. There is no segment-aware globbing for import paths — Rust path strings are short and the substring-with-wildcards model covers every current case.

#### 4.4.2 Filesystem-path globs (`layers.paths`, `layers.exempt_files`, `file_content_scan.glob`, `file_content_scan.exclude_glob`, `forbidden_inline_paths.in_files`, `forbidden_imports.in_files`, etc.)

Standard shell-style globs via the `globset` crate:

- `*` matches any sequence within one path segment.
- `**` matches any number of path segments.
- `?` matches one character.
- Globs are evaluated relative to `src_root` (or, for `fs_must_*`, the project root).

Examples: `app/**`, `transports/web/templates/**`, `src/transports/**/*.rs`, `transports/cli/commands/**`.

#### 4.4.3 Identifier patterns (`required_struct_attrs.struct_name_pattern`)

`*` matches any sequence of characters. `**` is not meaningful and is rejected with a config error. `Stub*` matches `Stub`, `StubFoo`, `Stubby`. Bare names with no `*` require an exact match.

## 5. Rule Catalog Migration

Every current arch-lint rule maps to one or more entries in `trammel.toml`:

| arch-lint rule | trammel rule kind(s) |
|---|---|
| `LIB_IS_FILE` (must exist) | `fs_must_exist` |
| `LIB_IS_FILE` (must not exist as dir) | `fs_must_not_exist` |
| `NO_APP_STATE_RS` | `fs_must_not_exist` |
| `NO_ADAPTERS_DIR` | `fs_must_not_exist` |
| `NO_EXTRACTORS_DIR` | `fs_must_not_exist` |
| `TEMPLATE_NO_PERSONA_CHECK` | `file_content_scan` |
| `TRANSPORT_NO_IS_ADMIN` | `file_content_scan` |
| `APP_NO_AXUM` | `forbidden_imports` |
| `APP_NO_TRANSPORTS` | `forbidden_imports` |
| `APP_NO_SQLX` | `forbidden_imports` + `forbidden_macros` |
| `SYSTEM_NO_AXUM` | `forbidden_imports` |
| `SYSTEM_NO_SQLX` | `forbidden_imports` + `forbidden_macros` (with `system/connectors/db.rs` as an `exempt_files` entry on the system layer) |
| `TRANSPORTS_NO_DB` | `forbidden_imports` + `forbidden_inline_paths` |
| `TRANSPORTS_NO_SQLX` | `forbidden_imports` + `forbidden_macros` (with `transports/web/middleware.rs` as an `exempt_files` entry on the transports_web layer) |
| `NO_DIRECT_EXTRACTORS` | `forbidden_imports` |
| `CLI_NO_DB` | `forbidden_imports` (scoped by `in_files = ["src/transports/cli/commands/**"]` if needed) |
| `ROUTER_NO_APP` | `forbidden_inline_paths` (with `in_files`) |
| `ANY_X_ONLY_IN_APP_OR_SYSTEM` | `forbidden_inline_paths` (with `allow_in_test_context = true`) |
| `NO_UNWRAP_IN_PRODUCTION` | `forbidden_methods` |
| `STUBS_MUST_BE_GATED` | `required_struct_attrs` (with `also_apply_to_impls = true`) |
| `N_PLUS_ONE` | `n_plus_one` |

arch-lint defines 20 distinct rule codes; the table above has more rows because some codes (e.g., `APP_NO_SQLX`, `SYSTEM_NO_SQLX`, `TRANSPORTS_NO_SQLX`) are enforced in arch-lint by both the import check and the macro check, and `LIB_IS_FILE` is enforced by both must-exist and must-not-exist-as-dir checks.

Acceptance criterion for the migration: north-star's `trammel.toml` reproduces every current arch-lint check, and `trammel check` against current main produces zero violations.

**Note for `TRANSPORTS_NO_SQLX`**: the `transports/web/middleware.rs` exemption applies to **both** the import check and the inline-path check; expressing it as an `exempt_files` entry on the `transports_web` layer (§4.2) covers both, since exempt files are skipped by every per-layer rule.

## 6. CLI Shape

```
trammel check                            # default action; reads ./trammel.toml
trammel check --config path/to.toml      # alternate config
trammel check --src custom/src           # alternate src_root (overrides config)

trammel rules                            # alias for `trammel rules list`
trammel rules list                       # enumerate all active rules from config
trammel rules explain RULE_NAME          # print rule's message + originating config

trammel --help
trammel --version
```

`check` is the primary action and a top-level verb. `rules` is a noun group (introspection commands). Future expansion (e.g., `trammel layers list` to print path classification) follows the noun-first shape.

Output on violations (preserves arch-lint's structural shape — per-rule headers, `file:line — message` lines, summary footer — with the prefix renamed from `arch-lint` to `trammel`):

```
── APP_NO_AXUM (2 violations) ──
  src/app/foo.rs:14 — app/ imports axum (`use axum::http`).
  src/app/bar.rs:7 — app/ imports axum (`use axum::Json`).

trammel: FAILED — 2 violations found.
```

Exit codes: 0 clean, 1 violations, 2 config / IO error.

**Subcommand routing.** `trammel rules` (no further arg) is implemented as a clap subcommand whose default action delegates to `trammel rules list`. Equivalent on the implementation side to a clap `arg_required_else_help = false` with a default-set inner subcommand.

## 7. Distribution & Licensing

- **License:** Apache 2.0. `LICENSE` file at repo root, SPDX `Apache-2.0` in each `Cargo.toml`. Copyright header in source files: `// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.`
- **NOTICE:** standard Apache NOTICE file naming Riverline Labs LLC.
- **Publication:** publish `trammel` and `trammel-attrs` to crates.io at v0.1.0 once tests are green. crates.io is the simplest dependency story for north-star and any future consumers.
- **CI:** GitHub Actions, `.github/workflows/ci.yml`, runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` on Rust stable. Triggers on PR and on push to `main`.

## 8. North-Star Migration

Performed in a single PR after `trammel` v0.1.0 is published.

1. **Replace the attribute crate.** Add `trammel-attrs = "0.1"` to the workspace `Cargo.toml`. Replace every `use arch_lint_attrs::allow_n_plus_one;` with `use trammel_attrs::allow_n_plus_one;`. Delete `crates/arch-lint-attrs/`.
2. **Add the config.** Create `trammel.toml` at the repo root capturing all 20 current arch-lint rule codes per the mapping in §5. The `N_PLUS_ONE` message text references the opt-out attribute by its new name (`trammel_attrs::allow_n_plus_one`); other messages are reproduced verbatim or paraphrased as the rule author prefers (the message field is config-owned). Verify `trammel check` produces zero violations.
3. **Switch the gate.**
    - `Makefile`: rename target `arch-lint:` to `trammel:`; replace command with `trammel check`. Update `verify` target's dependency list.
    - `.git/hooks/pre-push`: replace `cargo run -p arch-lint -- --src src` with `trammel check`. (Trammel must be on `$PATH`; documented in `README.md`.)
    - `.github/workflows/ci.yml`: replace the arch-lint step with a `cargo install --locked trammel` step followed by `trammel check`.
4. **Delete the binary crate.** Remove `crates/arch-lint/` and its workspace member entry. Remove from any remaining doc references.

Acceptance: pre-push runs clean against current main. `cargo build`, `cargo clippy`, `cargo test` remain green. No `arch_lint*` identifier remains in the tree.

## 9. Implementation Notes

### 9.1 Engine architecture

The engine driver mirrors today's arch-lint: `walkdir` + `syn::parse_file` per `.rs` file, plus a separate filesystem-only and content-scan pass. The single `ArchVisitor` becomes a `Visitor` that holds a reference to the parsed `Config` and dispatches each AST callback to every rule whose `in_layers` / `in_files` matches the current file. Rule modules expose a small `Rule` trait (`fn matches_use(...)`, `fn matches_method_call(...)`, etc.) but the trait is internal — there is no plugin mechanism for v0.1.0.

Test-context propagation (current behavior preserved): `#[test]`, `#[cfg(test)]`, `#[cfg(any(test, ...))]` on `fn`, `impl`, or `mod` push the `in_test_context` flag during the subtree walk. Layer-level `implicit_test_context = true` initializes the flag on entry to the layer.

N+1 detection inherits today's loop-depth tracker, the iteration-combinator list, and the recursion guard that prevents counting `.await`s in nested `fn` / `impl` items as inside the outer loop.

### 9.2 Glob matching

Use the `globset` crate for filesystem globs (file paths and glob-form scopes) and a small hand-rolled matcher for `::`-separated import-path globs and identifier-name globs. Stay dependency-light: `syn` (full + visit + extra-traits + span-locations on proc-macro2), `walkdir`, `globset`, `serde`, `toml`, `anyhow`, `clap` (for the CLI binary only).

### 9.3 Output stability

Output structure (per-rule header, `file:line — message`, footer with count) preserves arch-lint's shape so existing greps and log filters keep working. The prefix changes from `arch-lint:` to `trammel:`; CI log assertions matching the prefix must update during the migration step in §8.

## 10. Open Questions

None blocking. All design decisions are recorded above.
