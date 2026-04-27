# trammel

Config-driven architectural conformance for Rust workspaces.

## What it is

`trammel` parses your Rust source with `syn`, classifies each file into a
named layer per `trammel.toml`, and runs a small fixed set of rule kinds:
forbidden imports, forbidden inline paths, forbidden macros, forbidden
methods, required struct attributes, n+1 detection, filesystem layout
assertions, and content scans. It's a single binary plus a config file.

No editor integration, no rule DSL, no severity levels, no per-line
suppression comments — only the typed `#[allow_n_plus_one]` opt-out from the
companion `trammel-attrs` crate.

## Install

```sh
cargo install --locked trammel
```

For the n+1 opt-out attribute, add to your project:

```toml
[dependencies]
trammel-attrs = "0.1"
```

## Run

```sh
trammel check                            # reads ./trammel.toml
trammel check --config path/to.toml      # alternate config
trammel check --src custom/src           # override src_root from config

trammel rules                            # alias for `rules list`
trammel rules list                       # enumerate active rules
trammel rules explain RULE_NAME          # print a rule's message + scope
```

Exit codes: `0` clean, `1` violations, `2` config or IO error.

Output on violations preserves a consistent shape:

```
── APP_NO_AXUM (2 violations) ──
  src/app/foo.rs:14 — app/ imports axum (`use axum::http`).
  src/app/bar.rs:7  — app/ imports axum (`use axum::Json`).

trammel: FAILED — 2 violations found.
```

## Configuration

Single file, `trammel.toml`, at the project root.

### Top level

```toml
src_root = "src"   # optional; default "src"; relative to CWD
```

### Layers

A layer is a named set of paths under `src_root`. Path classification is
**first-match in declaration order**; a file matching no layer is skipped by
per-layer rules.

```toml
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "system"
paths = ["system/**"]
exempt_files = ["system/connectors/db.rs"]   # rules skip these files

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]
exempt_files = ["transports/web/middleware.rs"]

[[layers]]
name = "tests"
paths = ["tests.rs", "tests/**"]
implicit_test_context = true                 # treat layer as cfg(test)
```

`exempt_files` files are skipped by every rule scoped to that layer.
`implicit_test_context = true` makes every file in the layer behave as if it
were inside `#[cfg(test)]`.

**Test-context propagation.** A file is in test context when its layer has
`implicit_test_context = true`, OR the current AST node is inside a `#[test]`
fn, a `#[cfg(test)]` / `#[cfg(any(test, ...))]` `fn` / `impl` / `mod`. Rules
with `allow_in_test_context = true` skip violations in that context.

### Scoping

Most rule entries scope to layers (`in_layers = […]`) or to specific files
(`in_files = […]` — filesystem globs relative to `src_root`). Every rule
entry that supports scoping must declare at least one of `in_layers` or
`in_files`; omitting both is a config error. The exceptions are
`fs_must_exist`, `fs_must_not_exist`, and `file_content_scan`, which scope
by path glob directly.

### Rule kinds

#### `forbidden_imports`

Flag `use` statements matching configured patterns.

```toml
[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*", "*transports*", "sqlx*"]
rule = "APP_BOUNDARY"
message = "app/ must not import {pattern}"
```

`patterns` are import-path globs (see [Glob semantics](#glob-semantics)).
The `{pattern}` token in `message` is substituted with the offending pattern.

#### `forbidden_inline_paths`

Flag fully-qualified paths in expression or type position (e.g.
`crate::db::Foo` in a body, or `db::User` as a type annotation).

```toml
[[forbidden_inline_paths]]
in_layers = ["transports_web", "transports_cli"]
patterns = ["db", "db::*", "crate::db*", "sqlx", "sqlx::*"]
position = "any"                              # "expr" | "type" | "any" (default)
rule = "TRANSPORTS_NO_DB"
allow_in_test_context = true

[[forbidden_inline_paths]]
in_files = ["src/transports/web/router.rs"]
patterns = ["app::*"]
position = "expr"
rule = "ROUTER_NO_APP"
```

#### `forbidden_macros`

Flag macro invocations.

```toml
[[forbidden_macros]]
in_layers = ["app", "system", "transports_web", "transports_cli"]
qualified_names = ["sqlx::*"]                 # full path; import-path globs
bare_names = ["query", "query_as", "query_scalar", "query_unchecked"]
bare_names_in_layers = ["app", "system"]      # bare names flagged here only
rule = "NO_SQL_OUTSIDE_DB"
allow_in_test_context = true
```

`qualified_names` matches the joined `::`-separated path. `bare_names`
matches the macro's final segment by exact name (no globs) and is flagged
only in `bare_names_in_layers`.

#### `forbidden_methods`

Flag `.method()` calls.

```toml
[[forbidden_methods]]
in_layers = ["app", "system", "db", "transports_web", "transports_cli"]
methods = ["unwrap", "expect"]
allow_in_test_context = true
rule = "NO_UNWRAP_IN_PRODUCTION"
```

#### `required_struct_attrs`

Require structs (and optionally their `impl` blocks) matching a name pattern
to carry one of the listed attributes.

```toml
[[required_struct_attrs]]
in_layers = ["system"]
struct_name_pattern = "Stub*"                 # identifier glob
required_any_of = [
  "cfg(test)",
  'cfg(any(test, feature = "testing"))',
]
also_apply_to_impls = true
rule = "STUBS_MUST_BE_GATED"
```

Attribute matching is loose: an item satisfies `required_any_of` if any of
its attributes' stringified token trees (whitespace-normalized) **contains**
any of the required substrings. So `#[cfg(any(test, feature = "testing"))]`
satisfies a `cfg(test)` requirement.

When `also_apply_to_impls = true`, `struct_name_pattern` is matched against
the stringified `self_ty` of the `impl` block. The trait path (the `Foo` in
`impl Foo for X`) is not consulted.

#### `fs_must_exist` / `fs_must_not_exist`

Filesystem layout assertions. Paths are relative to the **project root**
(not `src_root`).

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
]
rule = "FORBIDDEN_PATHS"
```

`fs_must_exist` takes one `path` per entry. `fs_must_not_exist` accepts a
single `path` or a list `paths`.

#### `file_content_scan`

Substring scan of file contents under a glob — the right tool for non-Rust
files (templates) and rendered method calls regardless of qualification.

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

Line numbers are reported as `0` (file-level).

#### `n_plus_one`

Loop or iterator-combinator + `.await` on a configured DB-touching
expression. One `[n_plus_one]` table per project.

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
layer_assumes_query = ["db"]                  # in db/, every await is a query
rule = "N_PLUS_ONE"
```

**Detection.** `loop_depth` increments on `for` / `while` / `loop` and on
any method call whose name is in `combinators`. On every `.await` where
`loop_depth > 0` and the file's layer is in `in_layers`:

- if the layer is in `layer_assumes_query`, it is a violation unconditionally;
- otherwise, the awaited expression's subtree is scanned for any path
  matching `db_path_patterns` or any macro whose final segment matches a
  `db_macros` entry by exact name.

The subtree scan does not recurse into nested `fn` / `impl` items.
`loop_depth` is reset on entry to a nested `fn`. Test code (per
`implicit_test_context`, `#[test]`, or any enclosing `#[cfg(test)]`) is
exempt unconditionally.

**Opt-out.** `opt_out_attribute` matches the **final segment** of any
attribute on the enclosing fn — both `#[allow_n_plus_one]` and
`#[trammel_attrs::allow_n_plus_one]` work.

### Glob semantics

Three distinct match domains.

**Import-path patterns** — used in `forbidden_imports.patterns`,
`forbidden_inline_paths.patterns`, `forbidden_macros.qualified_names`, and
`n_plus_one.db_path_patterns`. `*` matches any sequence of characters,
**including `::` separators**. A bare pattern with no `*` is exact-match.
Examples:

| Pattern | Matches |
|---|---|
| `db` | exactly `db` |
| `db::*` | `db::User`, `db::queries::find` |
| `axum*` | `axum`, `axum::http`, `axum::extract::Json` |
| `*transports*` | `crate::transports::web`, `transports::extractors` |
| `crate::db*` | `crate::db`, `crate::db::User` |
| `*::db::*` | `crate::db::User`, `super::db::queries` |

**Filesystem-path globs** — used in `layers.paths`, `layers.exempt_files`,
`*.in_files`, `file_content_scan.glob` / `exclude_glob`. Standard shell
globs via the `globset` crate: `*` within a segment, `**` across segments,
`?` for one character. Evaluated relative to `src_root` (or project root
for `fs_must_*`).

**Identifier patterns** — used in `required_struct_attrs.struct_name_pattern`.
`*` matches any sequence of characters within an identifier. `**` is rejected
as a config error. Bare names are exact-match.

## License

Apache 2.0. See `LICENSE` and `NOTICE`.
