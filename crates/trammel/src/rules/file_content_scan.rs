// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `file_content_scan`: substring scan over file contents.
//!
//! Right tool for non-Rust files (templates) and for catching rendered method
//! calls regardless of qualification. Line numbers are reported as `0` —
//! file-level — since substring offsets aren't roundtripped to lines in v0.1.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use globset::Glob;
use walkdir::WalkDir;

use crate::config::Config;
use crate::violations::Violation;

pub fn check(cfg: &Config, project_root: &Path, violations: &mut Vec<Violation>) -> Result<()> {
    for rule in &cfg.file_content_scan {
        let include = Glob::new(&rule.glob)
            .with_context(|| format!("invalid glob `{}`", rule.glob))?
            .compile_matcher();
        let exclude = match &rule.exclude_glob {
            Some(g) => Some(
                Glob::new(g)
                    .with_context(|| format!("invalid exclude_glob `{g}`"))?
                    .compile_matcher(),
            ),
            None => None,
        };

        for entry in WalkDir::new(project_root)
            .follow_links(false)
            .into_iter()
            .filter_map(|r| r.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let abs = entry.path();
            let rel = match abs.strip_prefix(project_root) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if !include.is_match(&rel_str) {
                continue;
            }
            if let Some(ex) = &exclude {
                if ex.is_match(&rel_str) {
                    continue;
                }
            }
            let contents = match fs::read_to_string(abs) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for needle in &rule.forbidden_substrings {
                if contents.contains(needle) {
                    let message = match &rule.message {
                        Some(t) => t.replace("{substring}", needle).replace("{path}", &rel_str),
                        None => format!("file contains forbidden substring `{needle}`"),
                    };
                    violations.push(Violation {
                        file: abs.to_path_buf(),
                        line: 0,
                        rule: rule.rule.clone(),
                        message,
                    });
                }
            }
        }
    }
    Ok(())
}
