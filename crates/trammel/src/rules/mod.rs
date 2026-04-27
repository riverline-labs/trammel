// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Rule modules + the cross-cutting scope matcher.
//!
//! Each rule kind has its own module exposing a `check_*` entry that the
//! [`crate::visitor::Visitor`] calls at the matching `visit_*` callback.
//!
//! [`CompiledRules`] precompiles each rule entry's `in_files` glob list into
//! a [`GlobSet`] once per `run()` so the per-file loop never recompiles.

pub mod forbidden_imports;
pub mod forbidden_inline_paths;
pub mod forbidden_macros;
pub mod forbidden_methods;
pub mod path_string;
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
    pub forbidden_macros: Vec<GlobSet>,
    pub forbidden_methods: Vec<GlobSet>,
}

impl CompiledRules {
    pub fn build(cfg: &Config) -> Result<Self> {
        Ok(Self {
            forbidden_imports: build_each(cfg.forbidden_imports.iter().map(|r| &r.in_files))?,
            forbidden_inline_paths: build_each(
                cfg.forbidden_inline_paths.iter().map(|r| &r.in_files),
            )?,
            forbidden_macros: build_each(cfg.forbidden_macros.iter().map(|r| &r.in_files))?,
            forbidden_methods: build_each(cfg.forbidden_methods.iter().map(|r| &r.in_files))?,
        })
    }
}

fn build_each<'a, I>(iter: I) -> Result<Vec<GlobSet>>
where
    I: Iterator<Item = &'a Vec<String>>,
{
    iter.map(|files| fs_path::build_set(files)).collect()
}

/// Does a rule with the given `in_layers` / `in_files` apply to the current
/// file? OR semantics: layer match OR file match. (Validation already
/// guarantees at least one of the two is non-empty.)
pub fn scope_applies(
    in_layers: &[String],
    in_files: &GlobSet,
    layer: &Layer,
    rel_path: &str,
) -> bool {
    in_layers.iter().any(|n| n == &layer.name) || in_files.is_match(rel_path)
}
