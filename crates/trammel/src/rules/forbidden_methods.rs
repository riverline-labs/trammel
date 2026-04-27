// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `forbidden_methods`: flag `.method()` calls whose name is configured.

use crate::rules::scope_applies;
use crate::violations::Violation;
use crate::visitor::Visitor;

pub fn check(visitor: &mut Visitor, node: &syn::ExprMethodCall) {
    let cfg = visitor.cfg;
    let method = node.method.to_string();
    let line = node.method.span().start().line;

    for (idx, rule) in cfg.forbidden_methods.iter().enumerate() {
        let scope_files = &visitor.compiled.forbidden_methods[idx];
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
        if !rule.methods.iter().any(|m| m == &method) {
            continue;
        }
        let message = match &rule.message {
            Some(t) => t.replace("{method}", &method),
            None => format!("`.{method}()` is forbidden in this layer"),
        };
        visitor.violations.push(Violation {
            file: visitor.path.to_path_buf(),
            line,
            rule: rule.rule.clone(),
            message,
        });
        return;
    }
}
