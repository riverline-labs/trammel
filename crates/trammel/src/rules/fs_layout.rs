// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `fs_must_exist` / `fs_must_not_exist`: pre-AST filesystem layout
//! assertions. Paths are relative to the **project root**, not `src_root`.

use std::path::Path;

use crate::config::Config;
use crate::violations::Violation;

pub fn check(cfg: &Config, project_root: &Path, violations: &mut Vec<Violation>) {
    for rule in &cfg.fs_must_exist {
        let abs = project_root.join(&rule.path);
        if !abs.exists() {
            let message = match &rule.message {
                Some(t) => t.replace("{path}", &rule.path),
                None => format!("required path `{}` does not exist", rule.path),
            };
            violations.push(Violation {
                file: abs,
                line: 0,
                rule: rule.rule.clone(),
                message,
            });
        }
    }

    for rule in &cfg.fs_must_not_exist {
        let mut targets: Vec<&str> = rule.paths.iter().map(String::as_str).collect();
        if let Some(p) = rule.path.as_deref() {
            targets.push(p);
        }
        for path in targets {
            let abs = project_root.join(path);
            if abs.exists() {
                let message = match &rule.message {
                    Some(t) => t.replace("{path}", path),
                    None => format!("forbidden path `{path}` exists"),
                };
                violations.push(Violation {
                    file: abs,
                    line: 0,
                    rule: rule.rule.clone(),
                    message,
                });
            }
        }
    }
}
