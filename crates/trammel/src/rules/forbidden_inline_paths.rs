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
    check_path_at(visitor, &node.path, Position::Expr);
}

pub fn check_type(visitor: &mut Visitor, node: &syn::TypePath) {
    check_path_at(visitor, &node.path, Position::Type);
}

/// Generic dispatch: any callsite that has a `syn::Path` and knows its
/// position can route here. Used by `visit_expr_struct` (struct literals
/// like `crate::db::User { ... }`) since those don't go through `ExprPath`.
pub fn check_path_at(visitor: &mut Visitor, path: &syn::Path, site: Position) {
    let path_str = path_string::of(path);
    let line = path
        .segments
        .first()
        .map(|s| s.ident.span().start().line)
        .unwrap_or(0);
    check_inner(visitor, &path_str, line, site);
}

fn check_inner(visitor: &mut Visitor, path_str: &str, line: usize, site: Position) {
    let cfg = visitor.cfg;
    check_against(
        visitor,
        path_str,
        line,
        site,
        &cfg.forbidden_inline_paths,
        &visitor.compiled.forbidden_inline_paths,
    );
    check_against(
        visitor,
        path_str,
        line,
        site,
        &cfg.forbidden_constructors,
        &visitor.compiled.forbidden_constructors,
    );
}

fn check_against(
    visitor: &mut Visitor,
    path_str: &str,
    line: usize,
    site: Position,
    rules: &[crate::config::ForbiddenInlinePaths],
    compiled: &[globset::GlobSet],
) {
    for (idx, rule) in rules.iter().enumerate() {
        let scope_files = &compiled[idx];
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
