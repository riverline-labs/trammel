// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `forbidden_macros`: flag macro invocations whose path matches a
//! `qualified_names` glob, or whose final segment matches a `bare_names`
//! exact name (only in `bare_names_in_layers`).

use crate::glob::import_path;
use crate::rules::{path_string, scope_applies};
use crate::violations::Violation;
use crate::visitor::Visitor;

pub fn check(visitor: &mut Visitor, node: &syn::Macro) {
    let cfg = visitor.cfg;
    let qualified = path_string::of(&node.path);
    let bare = node
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    let line = node
        .path
        .segments
        .first()
        .map(|s| s.ident.span().start().line)
        .unwrap_or(0);

    for (idx, rule) in cfg.forbidden_macros.iter().enumerate() {
        let scope_files = &visitor.compiled.forbidden_macros[idx];
        if !scope_applies(
            &rule.in_layers,
            &rule.in_layers_except,
            scope_files,
            visitor.layer,
            visitor.rel_path,
        ) {
            continue;
        }
        if rule.allow_in_test_context && visitor.in_test_context {
            continue;
        }

        // Qualified-name glob match against the joined path.
        for pattern in &rule.qualified_names {
            if import_path::matches(pattern, &qualified) {
                visitor
                    .violations
                    .push(violation_for(visitor, rule, line, &qualified, pattern));
                return;
            }
        }

        // Bare-name exact match — only in configured layers, and only when the
        // macro is invoked unqualified (i.e. qualified path == bare ident).
        let is_bare_invocation = node.path.segments.len() == 1 && node.path.leading_colon.is_none();
        let layer_in_bare_scope = rule
            .bare_names_in_layers
            .iter()
            .any(|n| n == &visitor.layer.name);
        if is_bare_invocation && layer_in_bare_scope {
            for name in &rule.bare_names {
                if name == &bare {
                    visitor
                        .violations
                        .push(violation_for(visitor, rule, line, &bare, name));
                    return;
                }
            }
        }
    }
}

fn violation_for(
    visitor: &Visitor,
    rule: &crate::config::ForbiddenMacros,
    line: usize,
    matched: &str,
    pattern: &str,
) -> Violation {
    let message = match &rule.message {
        Some(t) => t.replace("{pattern}", pattern).replace("{macro}", matched),
        None => format!("macro `{matched}!` matches forbidden pattern `{pattern}`"),
    };
    Violation {
        file: visitor.path.to_path_buf(),
        line,
        rule: rule.rule.clone(),
        message,
    }
}
