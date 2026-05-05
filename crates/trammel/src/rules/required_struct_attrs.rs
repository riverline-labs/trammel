// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `required_struct_attrs`: structs (and optionally their `impl` blocks)
//! whose name matches `struct_name_pattern` must carry an attribute whose
//! ws-normalized token tree contains at least one of `required_any_of`.

use quote::ToTokens;

use crate::glob::ident;
use crate::rules::scope_applies;
use crate::violations::Violation;
use crate::visitor::Visitor;

pub fn check_struct(visitor: &mut Visitor, node: &syn::ItemStruct) {
    let cfg = visitor.cfg;
    let name = node.ident.to_string();
    let line = node.ident.span().start().line;

    for (idx, rule) in cfg.required_struct_attrs.iter().enumerate() {
        let scope_files = &visitor.compiled.required_struct_attrs[idx];
        if !scope_applies(
            &rule.in_layers,
            &rule.in_layers_except,
            scope_files,
            visitor.layer,
            visitor.rel_path,
        ) {
            continue;
        }
        if !ident::matches(&rule.struct_name_pattern, &name) {
            continue;
        }
        if attrs_satisfy(&node.attrs, &rule.required_any_of) {
            continue;
        }
        let message = match &rule.message {
            Some(t) => t.replace("{name}", &name),
            None => format!(
                "`{name}` is missing a required attribute (one of: {})",
                rule.required_any_of.join(", ")
            ),
        };
        visitor.violations.push(Violation {
            file: visitor.path.to_path_buf(),
            line,
            rule: rule.rule.clone(),
            message,
        });
    }
}

pub fn check_impl(visitor: &mut Visitor, node: &syn::ItemImpl) {
    let cfg = visitor.cfg;
    let self_ty_str = normalize_ws(&node.self_ty.to_token_stream().to_string());
    let line = node.impl_token.span.start().line;

    for (idx, rule) in cfg.required_struct_attrs.iter().enumerate() {
        if !rule.also_apply_to_impls {
            continue;
        }
        let scope_files = &visitor.compiled.required_struct_attrs[idx];
        if !scope_applies(
            &rule.in_layers,
            &rule.in_layers_except,
            scope_files,
            visitor.layer,
            visitor.rel_path,
        ) {
            continue;
        }
        if !ident::matches(&rule.struct_name_pattern, &self_ty_str) {
            continue;
        }
        if attrs_satisfy(&node.attrs, &rule.required_any_of) {
            continue;
        }
        let message = match &rule.message {
            Some(t) => t.replace("{name}", &self_ty_str),
            None => format!(
                "`impl {self_ty_str}` is missing a required attribute (one of: {})",
                rule.required_any_of.join(", ")
            ),
        };
        visitor.violations.push(Violation {
            file: visitor.path.to_path_buf(),
            line,
            rule: rule.rule.clone(),
            message,
        });
    }
}

fn attrs_satisfy(attrs: &[syn::Attribute], required_any_of: &[String]) -> bool {
    attrs.iter().any(|attr| {
        let stringified = normalize_ws(&attr.to_token_stream().to_string());
        required_any_of
            .iter()
            .any(|needle| stringified.contains(&normalize_ws(needle)))
    })
}

/// Collapse runs of ASCII whitespace and trim. Lets a user write
/// `cfg(any(test, feature = "testing"))` and match a token-tree string that
/// proc-macro2 emits with spaces around punctuation.
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join("")
}

#[cfg(test)]
mod tests {
    use super::normalize_ws;

    #[test]
    fn ws_collapses() {
        assert_eq!(normalize_ws("cfg ( test )"), "cfg(test)");
        assert_eq!(
            normalize_ws("cfg(any(test, feature = \"testing\"))"),
            "cfg(any(test,feature=\"testing\"))"
        );
        assert_eq!(normalize_ws("  test\nfoo "), "testfoo");
    }
}
