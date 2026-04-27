// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Config-driven architectural conformance tool for Rust workspaces.
//!
//! See the README for the configuration schema. The high-level entry is
//! [`run`]: load a [`Config`] (via [`config::load`]), pass it to `run` with
//! the project root, and inspect the returned violations.

use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

pub mod config;
pub mod glob;
pub mod layers;
pub mod rules;
pub mod violations;
pub mod visitor;

pub use config::Config;
pub use violations::Violation;

use layers::LayerSet;
use rules::CompiledRules;

/// Validate the config, run the full conformance pipeline against
/// `project_root`, and return every violation found.
///
/// Pipeline order: fs_layout → file_content_scan → per-file AST walk under
/// `project_root.join(cfg.src_root)`. Files that don't classify into any
/// layer are skipped silently.
pub fn run(cfg: &Config, project_root: &Path) -> Result<Vec<Violation>> {
    config::validate(cfg).context("config failed validation")?;

    let layer_set = LayerSet::build(cfg).context("failed to build layer set")?;
    let compiled = CompiledRules::build(cfg).context("failed to compile rule scopes")?;

    let mut violations = Vec::new();

    rules::fs_layout::check(cfg, project_root, &mut violations);
    rules::file_content_scan::check(cfg, project_root, &mut violations)
        .context("file_content_scan failed")?;

    let src_root_abs = project_root.join(&cfg.src_root);
    if src_root_abs.is_dir() {
        for entry in WalkDir::new(&src_root_abs)
            .follow_links(false)
            .into_iter()
            .filter_map(|r| r.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let abs = entry.path();
            if abs.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            let rel = match abs.strip_prefix(&src_root_abs) {
                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            let Some(layer) = layer_set.classify(&rel) else {
                continue;
            };
            let source = match std::fs::read_to_string(abs) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("trammel: failed to read {}: {e}", abs.display());
                    continue;
                }
            };
            let ast = match syn::parse_file(&source) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("trammel: failed to parse {}: {e}", abs.display());
                    continue;
                }
            };
            visitor::check_file(
                cfg,
                &layer_set,
                &compiled,
                abs,
                &rel,
                layer,
                &ast,
                &mut violations,
            );
        }
    }

    Ok(violations)
}
