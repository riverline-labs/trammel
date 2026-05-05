# Changelog

All notable changes to this project will be documented in this file. Format
follows [keep-a-changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.2.0] – 2026-05-05

### Added
- `in_layers_except` — inverse layer scope on `forbidden_imports`,
  `forbidden_inline_paths`, `forbidden_constructors`, `forbidden_macros`,
  `forbidden_methods`, and `required_struct_attrs`. Lets you say "every
  layer except these" without enumerating, so adding a new layer doesn't
  silently weaken determinism rules. Mutually exclusive with `in_layers`
  on a single entry. At least one of `in_layers`, `in_layers_except`, or
  `in_files` is still required.
- `forbidden_constructors` — naming alias for `forbidden_inline_paths`
  aimed at constructor-style patterns (`Uuid::new_v4`, `Utc::now`,
  `OsRng::default`). Same engine, separate slot so `trammel rules list`
  reports the kind authors wrote.
- `trammel check --json` — machine-readable output of violations as a
  JSON array with stable keys (`file`, `line`, `rule`, `message`). Empty
  input renders `[]`. Exit code unchanged.
- `trammel inspect <file>` — show how a file would be classified
  (layer, exempt status, test context) and which rule entries apply
  vs. would skip it. Existence is not checked, so hypothetical paths
  are valid input.
- `trammel rules` now accepts `--config`.
- README cookbook section: right-tool guidance for panic-class macros,
  determinism leaves, role gates, and `as` casts — based on patterns
  observed in real configs.

## [0.1.3] – 2026-04-27

### Fixed
- The `n_plus_one` opt-out attribute (e.g.
  `#[trammel_attrs::allow_n_plus_one]`) now suppresses violations when
  applied to **impl methods** and **trait methods with a default body**,
  not just freestanding `fn` items. Previously the visitor only
  overrode `visit_item_fn` for the propagation, so an annotation on
  `impl Foo { #[allow_n_plus_one] async fn bar() { ... } }` was
  silently ignored — the attribute compiled but did nothing. Now
  `visit_impl_item_fn` and `visit_trait_item_fn` apply the same opt-out,
  test-context, and `loop_depth` reset propagation as `visit_item_fn`.

## [0.1.2] – 2026-04-27

### Fixed
- `n_plus_one` no longer false-positives when an iterator combinator
  (`.map`, `.then`, `for_each`, …) is *chained after* the `.await`.
  Previously the visitor incremented `loop_depth` for the entire method
  call — receiver included — so a non-iterating shape like
  `db::fetch().await.into_iter().map(|h| h.id).collect()` was flagged
  as N+1 even though the combinator only post-processes the awaited
  `Vec`. The receiver of a combinator is now walked at the outer
  `loop_depth`; only its arguments (the closures that actually iterate)
  see the bumped depth. True N+1 shapes — `.await` lexically *inside*
  the combinator's closure or inside an enclosing `for` / `while` /
  `loop` — continue to fire.

## [0.1.1] – 2026-04-27

### Fixed
- `forbidden_inline_paths` now catches struct-literal paths (e.g.
  `crate::db::User { name: "x" }`). Previously these lived on
  `ExprStruct.path` (a raw `syn::Path`) and were missed by the
  `visit_expr_path` dispatch. The visitor now also overrides
  `visit_expr_struct` to dispatch the path at expression position.
- Test-context propagation now recognizes any attribute whose path's
  *final segment* is `test` — covers `#[tokio::test]`,
  `#[async_std::test]`, `#[sqlx::test]`, etc., not just bare `#[test]`.

## [0.1.0] – 2026-04-27

### Added
- Initial release. Two crates: `trammel` (lib + bin) and `trammel-attrs`
  (proc-macro).
- `trammel.toml` schema with eight rule kinds:
  `forbidden_imports`, `forbidden_inline_paths`, `forbidden_macros`,
  `forbidden_methods`, `required_struct_attrs`, `fs_must_exist` /
  `fs_must_not_exist`, `file_content_scan`, `n_plus_one`.
- Three glob domains: import-path (`*` crosses `::`), filesystem (via
  `globset`), identifier (`**` rejected as a config error).
- Layer model with first-match path classification, per-layer `exempt_files`,
  and `implicit_test_context`.
- Test-context propagation through `#[test]`, `#[cfg(test)]`,
  `#[cfg(any(test, ...))]` on `fn` / `impl` / `mod`, with per-rule
  `allow_in_test_context` opt-in.
- N+1 detection: loop or iterator-combinator + `.await` on a configured
  DB-touching expression. Per-fn opt-out via `#[allow_n_plus_one]` (the
  `trammel-attrs` crate exports this no-op marker; both bare and qualified
  forms match).
- CLI: `trammel check`, `trammel rules list`, `trammel rules explain RULE`.
- Output format preserves arch-lint's grouped-by-rule shape with the
  `trammel:` prefix.
