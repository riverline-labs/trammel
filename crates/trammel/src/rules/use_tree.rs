// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Expand a `syn::UseTree` into the individual fully-qualified paths it
//! introduces. This expands groups so each path is checked against patterns
//! independently.
//!
//! Examples:
//! - `use axum::http::StatusCode;` → `["axum::http::StatusCode"]`
//! - `use axum::{Json, http};` → `["axum::Json", "axum::http"]`
//! - `use axum::extract::*;` → `["axum::extract::*"]` (the literal `*`)
//! - `use db::User as Person;` → `["db::User"]` (the alias is irrelevant for
//!   import boundary checks)

use syn::UseTree;

pub fn paths(tree: &UseTree) -> Vec<String> {
    let mut out = Vec::new();
    expand(tree, String::new(), &mut out);
    out
}

fn expand(tree: &UseTree, prefix: String, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(p) => {
            let new_prefix = if prefix.is_empty() {
                p.ident.to_string()
            } else {
                format!("{prefix}::{}", p.ident)
            };
            expand(&p.tree, new_prefix, out);
        }
        UseTree::Name(n) => {
            out.push(join(&prefix, &n.ident.to_string()));
        }
        UseTree::Rename(r) => {
            out.push(join(&prefix, &r.ident.to_string()));
        }
        UseTree::Glob(_) => {
            out.push(if prefix.is_empty() {
                "*".to_string()
            } else {
                format!("{prefix}::*")
            });
        }
        UseTree::Group(g) => {
            for item in &g.items {
                expand(item, prefix.clone(), out);
            }
        }
    }
}

fn join(prefix: &str, leaf: &str) -> String {
    if prefix.is_empty() {
        leaf.to_string()
    } else {
        format!("{prefix}::{leaf}")
    }
}

#[cfg(test)]
mod tests {
    use super::paths;

    fn parse_tree(src: &str) -> syn::ItemUse {
        syn::parse_str(src).unwrap()
    }

    #[test]
    fn simple() {
        let item = parse_tree("use axum::http::StatusCode;");
        assert_eq!(paths(&item.tree), vec!["axum::http::StatusCode"]);
    }

    #[test]
    fn group_expands() {
        let item = parse_tree("use axum::{Json, http};");
        assert_eq!(paths(&item.tree), vec!["axum::Json", "axum::http"]);
    }

    #[test]
    fn nested_group_expands() {
        let item = parse_tree("use foo::{a::b, c::{d, e}};");
        assert_eq!(
            paths(&item.tree),
            vec!["foo::a::b", "foo::c::d", "foo::c::e"]
        );
    }

    #[test]
    fn rename_uses_original_path() {
        let item = parse_tree("use db::User as Person;");
        assert_eq!(paths(&item.tree), vec!["db::User"]);
    }

    #[test]
    fn glob_uses_star() {
        let item = parse_tree("use axum::extract::*;");
        assert_eq!(paths(&item.tree), vec!["axum::extract::*"]);
    }

    #[test]
    fn leading_crate() {
        let item = parse_tree("use crate::db::User;");
        assert_eq!(paths(&item.tree), vec!["crate::db::User"]);
    }
}
