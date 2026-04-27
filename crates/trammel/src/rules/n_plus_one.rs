// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `n_plus_one`: a `.await` inside a loop or iterator combinator on a
//! configured DB-touching expression.
//!
//! Loop tracking, the combinator list, and the `n_plus_one_suppressed` flag
//! are owned by [`crate::visitor::Visitor`]. This module decides whether
//! the awaited expression is a query, given the cfg.

use syn::visit::Visit;

use crate::config::NPlusOne;
use crate::glob::import_path;
use crate::rules::path_string;
use crate::violations::Violation;
use crate::visitor::Visitor;

pub fn check_await(visitor: &mut Visitor, node: &syn::ExprAwait) {
    let Some(n) = visitor.cfg.n_plus_one.as_ref() else {
        return;
    };
    if visitor.loop_depth == 0 {
        return;
    }
    if visitor.in_test_context {
        return;
    }
    if visitor.n_plus_one_suppressed {
        return;
    }
    if !n.in_layers.iter().any(|l| l == &visitor.layer.name) {
        return;
    }

    let is_query = if n
        .layer_assumes_query
        .iter()
        .any(|l| l == &visitor.layer.name)
    {
        true
    } else {
        expr_hits_db(&node.base, n)
    };
    if !is_query {
        return;
    }

    let line = node.await_token.span.start().line;
    let message = match &n.message {
        Some(t) => t.clone(),
        None => "`.await` on a db-touching call inside a loop or combinator. \
                 Each iteration issues a round-trip; batch with `WHERE id = ANY($1)`, \
                 a JOIN, or a multi-row INSERT. If batching is genuinely impossible \
                 (fan-out writes, polling, stream consumer), annotate the enclosing \
                 fn with the configured opt-out attribute."
            .to_string(),
    };
    visitor.violations.push(Violation {
        file: visitor.path.to_path_buf(),
        line,
        rule: n.rule.clone(),
        message,
    });
}

/// Scan an expression's subtree for any path matching `db_path_patterns`
/// (import-path glob) or any macro whose final segment matches `db_macros`
/// by exact name. Does NOT recurse into nested `fn` / `impl` items: their
/// bodies aren't executed by the outer `.await`.
fn expr_hits_db(expr: &syn::Expr, n: &NPlusOne) -> bool {
    struct Finder<'a> {
        n: &'a NPlusOne,
        found: bool,
    }
    impl<'ast, 'a> Visit<'ast> for Finder<'a> {
        fn visit_path(&mut self, path: &'ast syn::Path) {
            if self.found {
                return;
            }
            let s = path_string::of(path);
            if self
                .n
                .db_path_patterns
                .iter()
                .any(|p| import_path::matches(p, &s))
            {
                self.found = true;
                return;
            }
            syn::visit::visit_path(self, path);
        }
        fn visit_macro(&mut self, node: &'ast syn::Macro) {
            if self.found {
                return;
            }
            let final_segment = node
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            if self.n.db_macros.iter().any(|m| m == &final_segment) {
                self.found = true;
                return;
            }
            syn::visit::visit_macro(self, node);
        }
        fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
        fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {}
    }
    let mut f = Finder { n, found: false };
    f.visit_expr(expr);
    f.found
}
