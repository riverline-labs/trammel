// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `forbidden_imports`: flag `use` statements whose introduced paths match
//! configured import-path globs.

use syn::spanned::Spanned;

use crate::glob::import_path;
use crate::rules::{scope_applies, use_tree};
use crate::violations::Violation;
use crate::visitor::Visitor;

pub fn check(visitor: &mut Visitor, node: &syn::ItemUse) {
    let cfg = visitor.cfg;
    let line = node.use_token.span().start().line;
    let paths = use_tree::paths(&node.tree);

    for (idx, rule) in cfg.forbidden_imports.iter().enumerate() {
        let scope_files = &visitor.compiled.forbidden_imports[idx];
        if !scope_applies(
            &rule.in_layers,
            scope_files,
            visitor.layer,
            visitor.rel_path,
        ) {
            continue;
        }
        if rule.allow_in_test_context && visitor.in_test_context {
            continue;
        }
        for path_str in &paths {
            for pattern in &rule.patterns {
                if import_path::matches(pattern, path_str) {
                    let message = match &rule.message {
                        Some(t) => t.replace("{pattern}", pattern),
                        None => format!("`use {path_str}` matches forbidden pattern `{pattern}`"),
                    };
                    visitor.violations.push(Violation {
                        file: visitor.path.to_path_buf(),
                        line,
                        rule: rule.rule.clone(),
                        message,
                    });
                    break;
                }
            }
        }
    }
}
