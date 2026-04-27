// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! AST visitor scaffold.
//!
//! Drives a `syn::visit::Visit` traversal of one parsed `.rs` file, threading
//! the contextual flags every rule needs:
//!
//! - `in_test_context` — initialized from the layer's `implicit_test_context`,
//!   then OR'd by `#[test]` / `#[cfg(test)]` / `#[cfg(any(test, ...))]` on
//!   enclosing `fn` / `impl` / `mod` items, save/restore on exit.
//! - `loop_depth` — incremented on `for` / `while` / `loop` and on iterator
//!   combinator method calls (configured via `n_plus_one.combinators`).
//!   Saved and reset to 0 on entry to a nested `fn` (the closure or async fn
//!   defined inside a loop body executes its body separately, not on each
//!   outer iteration), restored on exit. Not reset on `impl` items because
//!   `impl` blocks contain item declarations, not executable scope.
//! - `n_plus_one_suppressed` — set when the enclosing `fn` carries the
//!   configured `n_plus_one.opt_out_attribute` (matched against the *final
//!   segment* so both `#[allow_n_plus_one]` and
//!   `#[trammel_attrs::allow_n_plus_one]` work). Save/restore on fn exit.
//!
//! Rule modules in `crate::rules::*` plug into the dispatch points; this
//! module owns only the propagation.

use std::path::Path;

use quote::ToTokens;
use syn::visit::Visit;

use crate::config::{Config, Layer};
use crate::layers::LayerSet;
use crate::violations::Violation;

/// Entry point: traverse one parsed file and append any violations found.
pub fn check_file<'a>(
    cfg: &'a Config,
    layer_set: &'a LayerSet<'a>,
    path: &'a Path,
    rel_path: &'a str,
    layer: &'a Layer,
    ast: &'a syn::File,
    violations: &mut Vec<Violation>,
) {
    if layer_set.is_exempt(layer, rel_path) {
        return;
    }
    let mut visitor = Visitor {
        cfg,
        layer_set,
        layer,
        path,
        rel_path,
        in_test_context: layer.implicit_test_context,
        loop_depth: 0,
        n_plus_one_suppressed: false,
        violations,
    };
    visitor.visit_file(ast);
}

pub struct Visitor<'a, 'v> {
    pub cfg: &'a Config,
    pub layer_set: &'a LayerSet<'a>,
    pub layer: &'a Layer,
    pub path: &'a Path,
    pub rel_path: &'a str,
    pub in_test_context: bool,
    pub loop_depth: usize,
    pub n_plus_one_suppressed: bool,
    pub violations: &'v mut Vec<Violation>,
}

impl<'a, 'v> Visitor<'a, 'v> {
    fn is_combinator(&self, name: &str) -> bool {
        self.cfg
            .n_plus_one
            .as_ref()
            .map(|n| n.combinators.iter().any(|c| c == name))
            .unwrap_or(false)
    }

    fn opt_out_name(&self) -> Option<&str> {
        self.cfg
            .n_plus_one
            .as_ref()
            .map(|n| n.opt_out_attribute.as_str())
    }
}

/// `#[test]`, `#[cfg(test)]`, or `#[cfg(any(test, ...))]` on the item.
fn item_is_test_scoped(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if attr.path().is_ident("cfg") {
            return attr.to_token_stream().to_string().contains("test");
        }
        false
    })
}

/// True if any attribute's *final* path segment equals `name`.
/// Lets both `#[allow_n_plus_one]` and `#[trammel_attrs::allow_n_plus_one]`
/// satisfy `name = "allow_n_plus_one"`.
fn has_attr_named(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .map(|s| s.ident == name)
            .unwrap_or(false)
    })
}

impl<'ast, 'a, 'v> Visit<'ast> for Visitor<'a, 'v> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let was_test = self.in_test_context;
        let was_suppressed = self.n_plus_one_suppressed;
        let saved_depth = self.loop_depth;

        if item_is_test_scoped(&node.attrs) {
            self.in_test_context = true;
        }
        if let Some(opt_out) = self.opt_out_name() {
            if has_attr_named(&node.attrs, opt_out) {
                self.n_plus_one_suppressed = true;
            }
        }

        self.loop_depth = 0;
        syn::visit::visit_item_fn(self, node);
        self.loop_depth = saved_depth;

        self.in_test_context = was_test;
        self.n_plus_one_suppressed = was_suppressed;
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let was = self.in_test_context;
        if item_is_test_scoped(&node.attrs) {
            self.in_test_context = true;
        }
        syn::visit::visit_item_impl(self, node);
        self.in_test_context = was;
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let was = self.in_test_context;
        if item_is_test_scoped(&node.attrs) {
            self.in_test_context = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test_context = was;
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.loop_depth += 1;
        syn::visit::visit_expr_for_loop(self, node);
        self.loop_depth -= 1;
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.loop_depth += 1;
        syn::visit::visit_expr_while(self, node);
        self.loop_depth -= 1;
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.loop_depth += 1;
        syn::visit::visit_expr_loop(self, node);
        self.loop_depth -= 1;
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let combinator = self.is_combinator(&node.method.to_string());
        if combinator {
            self.loop_depth += 1;
        }
        syn::visit::visit_expr_method_call(self, node);
        if combinator {
            self.loop_depth -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    //! These tests assert the *propagation* logic only — not any rule check.
    //! A `RecordingVisitor` wraps the real `Visitor` and snapshots the
    //! contextual flags every time a leaf method is visited.

    use super::*;
    use crate::config::Config;
    use std::cell::RefCell;
    use std::path::PathBuf;

    fn make_cfg() -> Config {
        toml::from_str(
            r#"
[[layers]]
name = "app"
paths = ["app/**"]

[n_plus_one]
in_layers = ["app"]
db_path_patterns = ["db::*"]
db_macros = []
combinators = ["map", "for_each", "then"]
opt_out_attribute = "allow_n_plus_one"
rule = "N_PLUS_ONE"
"#,
        )
        .unwrap()
    }

    /// Wraps the real visitor with a hook that snapshots the contextual
    /// flags whenever an `Ident` is visited (cheap leaf signal).
    struct RecordingVisitor<'a, 'v> {
        inner: Visitor<'a, 'v>,
        snapshots: RefCell<Vec<(String, bool, usize, bool)>>,
    }

    impl<'ast, 'a, 'v> Visit<'ast> for RecordingVisitor<'a, 'v> {
        fn visit_ident(&mut self, ident: &'ast syn::Ident) {
            self.snapshots.borrow_mut().push((
                ident.to_string(),
                self.inner.in_test_context,
                self.inner.loop_depth,
                self.inner.n_plus_one_suppressed,
            ));
        }
        fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
            // Drive the same propagation as the real visitor would.
            let was_test = self.inner.in_test_context;
            let was_suppressed = self.inner.n_plus_one_suppressed;
            let saved_depth = self.inner.loop_depth;

            if item_is_test_scoped(&node.attrs) {
                self.inner.in_test_context = true;
            }
            if let Some(opt_out) = self.inner.opt_out_name() {
                if has_attr_named(&node.attrs, opt_out) {
                    self.inner.n_plus_one_suppressed = true;
                }
            }
            self.inner.loop_depth = 0;
            syn::visit::visit_item_fn(self, node);
            self.inner.loop_depth = saved_depth;
            self.inner.in_test_context = was_test;
            self.inner.n_plus_one_suppressed = was_suppressed;
        }
        fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
            let was = self.inner.in_test_context;
            if item_is_test_scoped(&node.attrs) {
                self.inner.in_test_context = true;
            }
            syn::visit::visit_item_impl(self, node);
            self.inner.in_test_context = was;
        }
        fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
            let was = self.inner.in_test_context;
            if item_is_test_scoped(&node.attrs) {
                self.inner.in_test_context = true;
            }
            syn::visit::visit_item_mod(self, node);
            self.inner.in_test_context = was;
        }
        fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
            self.inner.loop_depth += 1;
            syn::visit::visit_expr_for_loop(self, node);
            self.inner.loop_depth -= 1;
        }
        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            let combinator = self.inner.is_combinator(&node.method.to_string());
            if combinator {
                self.inner.loop_depth += 1;
            }
            syn::visit::visit_expr_method_call(self, node);
            if combinator {
                self.inner.loop_depth -= 1;
            }
        }
    }

    fn drive(src: &str) -> Vec<(String, bool, usize, bool)> {
        let cfg = make_cfg();
        let layer_set = LayerSet::build(&cfg).unwrap();
        let layer = &cfg.layers[0];
        let path = PathBuf::from("app/test.rs");
        let mut violations = Vec::new();
        let ast = syn::parse_str::<syn::File>(src).unwrap();
        let inner = Visitor {
            cfg: &cfg,
            layer_set: &layer_set,
            layer,
            path: &path,
            rel_path: "app/test.rs",
            in_test_context: layer.implicit_test_context,
            loop_depth: 0,
            n_plus_one_suppressed: false,
            violations: &mut violations,
        };
        let mut rec = RecordingVisitor {
            inner,
            snapshots: RefCell::new(Vec::new()),
        };
        rec.visit_file(&ast);
        rec.snapshots.into_inner()
    }

    #[test]
    fn for_loop_increments_depth() {
        let snaps = drive(
            r#"
            fn outer() {
                let inside_outer = ();
                for x in xs {
                    let inside_loop = ();
                }
            }
            "#,
        );
        let outer = snaps
            .iter()
            .find(|(n, _, _, _)| n == "inside_outer")
            .unwrap();
        let inside = snaps
            .iter()
            .find(|(n, _, _, _)| n == "inside_loop")
            .unwrap();
        assert_eq!(outer.2, 0);
        assert_eq!(inside.2, 1);
    }

    #[test]
    fn nested_fn_resets_loop_depth() {
        let snaps = drive(
            r#"
            fn outer() {
                for x in xs {
                    fn inner() {
                        let inside_inner = ();
                    }
                    let inside_loop = ();
                }
            }
            "#,
        );
        let inside_inner = snaps
            .iter()
            .find(|(n, _, _, _)| n == "inside_inner")
            .unwrap();
        let inside_loop = snaps
            .iter()
            .find(|(n, _, _, _)| n == "inside_loop")
            .unwrap();
        assert_eq!(inside_inner.2, 0, "nested fn body starts at depth 0");
        assert_eq!(
            inside_loop.2, 1,
            "outer loop depth restored after nested fn"
        );
    }

    #[test]
    fn combinator_method_call_increments_depth() {
        let snaps = drive(
            r#"
            fn outer() {
                xs.iter().for_each(|x| {
                    let inside_combinator = ();
                });
            }
            "#,
        );
        let inside = snaps
            .iter()
            .find(|(n, _, _, _)| n == "inside_combinator")
            .unwrap();
        assert!(
            inside.2 >= 1,
            "for_each should bump depth, got {}",
            inside.2
        );
    }

    #[test]
    fn test_attr_propagates() {
        let snaps = drive(
            r#"
            fn prod_fn() {
                let in_prod = ();
            }
            #[test]
            fn test_fn() {
                let in_test = ();
            }
            "#,
        );
        let prod = snaps.iter().find(|(n, _, _, _)| n == "in_prod").unwrap();
        let in_test = snaps.iter().find(|(n, _, _, _)| n == "in_test").unwrap();
        assert!(!prod.1);
        assert!(in_test.1);
    }

    #[test]
    fn cfg_test_propagates() {
        let snaps = drive(
            r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let in_helper = ();
                }
            }
            fn prod() {
                let in_prod = ();
            }
            "#,
        );
        let helper = snaps.iter().find(|(n, _, _, _)| n == "in_helper").unwrap();
        let prod = snaps.iter().find(|(n, _, _, _)| n == "in_prod").unwrap();
        assert!(helper.1);
        assert!(!prod.1);
    }

    #[test]
    fn opt_out_attr_propagates_and_restores() {
        let snaps = drive(
            r#"
            #[allow_n_plus_one]
            fn suppressed() {
                let in_suppressed = ();
            }
            fn unsuppressed() {
                let in_unsuppressed = ();
            }
            "#,
        );
        let s = snaps
            .iter()
            .find(|(n, _, _, _)| n == "in_suppressed")
            .unwrap();
        let u = snaps
            .iter()
            .find(|(n, _, _, _)| n == "in_unsuppressed")
            .unwrap();
        assert!(s.3);
        assert!(!u.3);
    }

    #[test]
    fn opt_out_qualified_path_also_works() {
        let snaps = drive(
            r#"
            #[trammel_attrs::allow_n_plus_one]
            fn suppressed() {
                let in_suppressed = ();
            }
            "#,
        );
        let s = snaps
            .iter()
            .find(|(n, _, _, _)| n == "in_suppressed")
            .unwrap();
        assert!(s.3);
    }
}
