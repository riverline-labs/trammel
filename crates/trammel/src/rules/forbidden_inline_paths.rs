// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `forbidden_inline_paths`: flag fully-qualified path references in
//! expression position (e.g. `crate::db::Foo` in a body) and type position
//! (e.g. `db::User` as a type annotation).

use crate::config::Position;
use crate::glob::import_path;
use crate::rules::{path_string, scope_applies};
use crate::violations::Violation;
use crate::visitor::Visitor;

pub fn check_expr(visitor: &mut Visitor, node: &syn::ExprPath) {
    let path_str = path_string::of(&node.path);
    let line = node
        .path
        .segments
        .first()
        .map(|s| s.ident.span().start().line)
        .unwrap_or(0);
    check_inner(visitor, &path_str, line, Position::Expr);
}

pub fn check_type(visitor: &mut Visitor, node: &syn::TypePath) {
    let path_str = path_string::of(&node.path);
    let line = node
        .path
        .segments
        .first()
        .map(|s| s.ident.span().start().line)
        .unwrap_or(0);
    check_inner(visitor, &path_str, line, Position::Type);
}

fn check_inner(visitor: &mut Visitor, path_str: &str, line: usize, site: Position) {
    let cfg = visitor.cfg;
    for (idx, rule) in cfg.forbidden_inline_paths.iter().enumerate() {
        let scope_files = &visitor.compiled.forbidden_inline_paths[idx];
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
        if rule.position != Position::Any && rule.position != site {
            continue;
        }
        for pattern in &rule.patterns {
            if import_path::matches(pattern, path_str) {
                let message = match &rule.message {
                    Some(t) => t.replace("{pattern}", pattern).replace("{path}", path_str),
                    None => {
                        format!("inline path `{path_str}` matches forbidden pattern `{pattern}`")
                    }
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
