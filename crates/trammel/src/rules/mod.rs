// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Rule modules + the cross-cutting scope matcher.
//!
//! Each rule kind has its own module exposing a `check_*` entry that the
//! [`crate::visitor::Visitor`] calls at the matching `visit_*` callback.
//!
//! [`CompiledRules`] precompiles each rule entry's `in_files` glob list into
//! a [`GlobSet`] once per `run()` so the per-file loop never recompiles.

pub mod file_content_scan;
pub mod forbidden_imports;
pub mod forbidden_inline_paths;
pub mod forbidden_macros;
pub mod forbidden_methods;
pub mod fs_layout;
pub mod n_plus_one;
pub mod path_string;
pub mod required_struct_attrs;
pub mod use_tree;

use anyhow::Result;
use globset::GlobSet;

use crate::config::{Config, Layer};
use crate::glob::fs_path;

/// Pre-compiled `in_files` glob sets per rule entry, parallel-indexed to
/// each `cfg.<rule_kind>` vec.
pub struct CompiledRules {
    pub forbidden_imports: Vec<GlobSet>,
    pub forbidden_inline_paths: Vec<GlobSet>,
    pub forbidden_constructors: Vec<GlobSet>,
    pub forbidden_macros: Vec<GlobSet>,
    pub forbidden_methods: Vec<GlobSet>,
    pub required_struct_attrs: Vec<GlobSet>,
}

impl CompiledRules {
    pub fn build(cfg: &Config) -> Result<Self> {
        Ok(Self {
            forbidden_imports: build_each(cfg.forbidden_imports.iter().map(|r| &r.in_files))?,
            forbidden_inline_paths: build_each(
                cfg.forbidden_inline_paths.iter().map(|r| &r.in_files),
            )?,
            forbidden_constructors: build_each(
                cfg.forbidden_constructors.iter().map(|r| &r.in_files),
            )?,
            forbidden_macros: build_each(cfg.forbidden_macros.iter().map(|r| &r.in_files))?,
            forbidden_methods: build_each(cfg.forbidden_methods.iter().map(|r| &r.in_files))?,
            required_struct_attrs: build_each(
                cfg.required_struct_attrs.iter().map(|r| &r.in_files),
            )?,
        })
    }
}

fn build_each<'a, I>(iter: I) -> Result<Vec<GlobSet>>
where
    I: Iterator<Item = &'a Vec<String>>,
{
    iter.map(|files| fs_path::build_set(files)).collect()
}

/// Does a rule with the given layer/file scope apply to the current file?
///
/// OR semantics: layer match OR file match. Validation already guarantees
/// the rule declared at least one of `in_layers`, `in_layers_except`, or
/// `in_files`, and that `in_layers` and `in_layers_except` are not both set.
///
/// When `in_layers_except` is non-empty, the layer matches iff it is NOT
/// listed there. Otherwise the layer matches iff it IS listed in
/// `in_layers`.
pub fn scope_applies(
    in_layers: &[String],
    in_layers_except: &[String],
    in_files: &GlobSet,
    layer: &Layer,
    rel_path: &str,
) -> bool {
    let layer_match = if !in_layers_except.is_empty() {
        !in_layers_except.iter().any(|n| n == &layer.name)
    } else {
        in_layers.iter().any(|n| n == &layer.name)
    };
    layer_match || in_files.is_match(rel_path)
}

#[cfg(test)]
mod tests {
    //! Truth table for `scope_applies`. The visitor never sees `Layer`
    //! literals — only references to layers in the user's config — so the
    //! tests fabricate a layer here to exercise the matcher in isolation.

    use super::*;
    use crate::glob::fs_path;

    fn layer(name: &str) -> Layer {
        toml::from_str(&format!("name = \"{name}\"\npaths = [\"{name}/**\"]"))
            .expect("layer parses")
    }

    fn glob(patterns: &[&str]) -> GlobSet {
        let owned: Vec<String> = patterns.iter().map(|s| s.to_string()).collect();
        fs_path::build_set(&owned).expect("glob compiles")
    }

    fn s(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn in_layers_positive_match() {
        let l = layer("app");
        assert!(scope_applies(
            &s(&["app", "system"]),
            &[],
            &glob(&[]),
            &l,
            "app/foo.rs"
        ));
    }

    #[test]
    fn in_layers_layer_not_listed_does_not_match() {
        let l = layer("transports");
        assert!(!scope_applies(
            &s(&["app", "system"]),
            &[],
            &glob(&[]),
            &l,
            "transports/foo.rs"
        ));
    }

    #[test]
    fn in_layers_except_excluded_layer_does_not_match() {
        let l = layer("world");
        assert!(!scope_applies(
            &[],
            &s(&["world"]),
            &glob(&[]),
            &l,
            "world/clock.rs"
        ));
    }

    #[test]
    fn in_layers_except_other_layer_matches() {
        let l = layer("app");
        assert!(scope_applies(
            &[],
            &s(&["world"]),
            &glob(&[]),
            &l,
            "app/foo.rs"
        ));
    }

    #[test]
    fn in_layers_except_takes_precedence_over_in_layers() {
        // Validation rejects this combination, but the matcher should still
        // be deterministic if it ever sees both: except wins.
        let l = layer("world");
        assert!(!scope_applies(
            &s(&["world"]),
            &s(&["world"]),
            &glob(&[]),
            &l,
            "world/foo.rs"
        ));
    }

    #[test]
    fn in_files_match_succeeds_even_when_layer_does_not() {
        let l = layer("transports");
        assert!(scope_applies(
            &s(&["app"]),
            &[],
            &glob(&["transports/foo.rs"]),
            &l,
            "transports/foo.rs"
        ));
    }

    #[test]
    fn in_files_only_no_layer_match() {
        let l = layer("transports");
        // No in_layers / in_layers_except — purely file-scoped.
        assert!(scope_applies(
            &[],
            &[],
            &glob(&["transports/foo.rs"]),
            &l,
            "transports/foo.rs"
        ));
        assert!(!scope_applies(
            &[],
            &[],
            &glob(&["transports/foo.rs"]),
            &l,
            "transports/bar.rs"
        ));
    }

    #[test]
    fn empty_scope_matches_nothing() {
        // Validation forbids this, but the matcher should not match.
        let l = layer("app");
        assert!(!scope_applies(&[], &[], &glob(&[]), &l, "app/foo.rs"));
    }
}
