// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Config loading and validation.

pub mod schema;

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

pub use schema::{
    Config, FileContentScan, ForbiddenImports, ForbiddenInlinePaths, ForbiddenMacros,
    ForbiddenMethods, FsMustExist, FsMustNotExist, Layer, NPlusOne, Position, RequiredStructAttrs,
};

use crate::glob::ident;

/// Read and parse a `trammel.toml` from disk.
pub fn load(path: &Path) -> Result<Config> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file `{}`", path.display()))?;
    let cfg: Config = toml::from_str(&raw)
        .with_context(|| format!("failed to parse `{}` as trammel.toml", path.display()))?;
    Ok(cfg)
}

/// Reject any config the engine cannot soundly execute.
///
/// Catches: rule entries with neither `in_layers` nor `in_files`; layer names
/// referenced in rules that don't exist in `[[layers]]`; identifier patterns
/// containing `**`; `n_plus_one.layer_assumes_query` containing layers not in
/// `n_plus_one.in_layers`; and `fs_must_not_exist` entries with neither `path`
/// nor `paths`.
pub fn validate(cfg: &Config) -> Result<()> {
    let layer_names: HashSet<&str> = cfg.layers.iter().map(|l| l.name.as_str()).collect();

    let check_layer_names = |names: &[String], context: &str| -> Result<()> {
        for name in names {
            if !layer_names.contains(name.as_str()) {
                return Err(anyhow!(
                    "{context} references unknown layer `{name}` (declare it in [[layers]])"
                ));
            }
        }
        Ok(())
    };

    let require_scope = |in_layers: &[String], in_files: &[String], rule: &str| -> Result<()> {
        if in_layers.is_empty() && in_files.is_empty() {
            return Err(anyhow!(
                "rule `{rule}` must declare at least one of `in_layers` or `in_files`"
            ));
        }
        Ok(())
    };

    for r in &cfg.forbidden_imports {
        require_scope(&r.in_layers, &r.in_files, &r.rule)?;
        check_layer_names(&r.in_layers, &format!("forbidden_imports `{}`", r.rule))?;
    }
    for r in &cfg.forbidden_inline_paths {
        require_scope(&r.in_layers, &r.in_files, &r.rule)?;
        check_layer_names(
            &r.in_layers,
            &format!("forbidden_inline_paths `{}`", r.rule),
        )?;
    }
    for r in &cfg.forbidden_macros {
        require_scope(&r.in_layers, &r.in_files, &r.rule)?;
        check_layer_names(&r.in_layers, &format!("forbidden_macros `{}`", r.rule))?;
        check_layer_names(
            &r.bare_names_in_layers,
            &format!("forbidden_macros `{}` bare_names_in_layers", r.rule),
        )?;
    }
    for r in &cfg.forbidden_methods {
        require_scope(&r.in_layers, &r.in_files, &r.rule)?;
        check_layer_names(&r.in_layers, &format!("forbidden_methods `{}`", r.rule))?;
    }
    for r in &cfg.required_struct_attrs {
        require_scope(&r.in_layers, &r.in_files, &r.rule)?;
        check_layer_names(&r.in_layers, &format!("required_struct_attrs `{}`", r.rule))?;
        ident::validate(&r.struct_name_pattern).with_context(|| {
            format!(
                "required_struct_attrs `{}` has invalid struct_name_pattern",
                r.rule
            )
        })?;
    }
    for r in &cfg.fs_must_not_exist {
        if r.path.is_none() && r.paths.is_empty() {
            return Err(anyhow!(
                "fs_must_not_exist `{}` must declare at least one of `path` or `paths`",
                r.rule
            ));
        }
    }

    if let Some(n) = &cfg.n_plus_one {
        check_layer_names(&n.in_layers, "n_plus_one in_layers")?;
        let in_layers: HashSet<&str> = n.in_layers.iter().map(String::as_str).collect();
        for layer in &n.layer_assumes_query {
            if !in_layers.contains(layer.as_str()) {
                return Err(anyhow!(
                    "n_plus_one `layer_assumes_query` contains `{layer}` which is not in `in_layers`"
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> Config {
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn rejects_rule_with_no_scope() {
        let cfg = parse(
            r#"
[[forbidden_imports]]
patterns = ["axum*"]
rule = "NO_AXUM"
"#,
        );
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("NO_AXUM"));
        assert!(err.to_string().contains("in_layers"));
    }

    #[test]
    fn rejects_unknown_layer_name() {
        let cfg = parse(
            r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_imports]]
in_layers = ["does_not_exist"]
patterns = ["axum*"]
rule = "BAD"
"#,
        );
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("does_not_exist"));
    }

    #[test]
    fn rejects_double_star_in_struct_pattern() {
        let cfg = parse(
            r#"
[[layers]]
name = "system"
paths = ["system/**"]

[[required_struct_attrs]]
in_layers = ["system"]
struct_name_pattern = "Stub**"
required_any_of = ["cfg(test)"]
rule = "STUBS"
"#,
        );
        let err = validate(&cfg).unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("STUBS"), "missing rule name: {chain}");
        assert!(chain.contains("**"), "missing `**` mention: {chain}");
    }

    #[test]
    fn rejects_layer_assumes_query_not_in_in_layers() {
        let cfg = parse(
            r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "db"
paths = ["db/**"]

[n_plus_one]
in_layers = ["app"]
db_path_patterns = ["db::*"]
db_macros = []
combinators = []
opt_out_attribute = "allow_n_plus_one"
layer_assumes_query = ["db"]
rule = "N_PLUS_ONE"
"#,
        );
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("layer_assumes_query"));
        assert!(err.to_string().contains("db"));
    }

    #[test]
    fn rejects_fs_must_not_exist_without_path_or_paths() {
        let cfg = parse(
            r#"
[[fs_must_not_exist]]
rule = "BAD"
"#,
        );
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("BAD"));
    }

    #[test]
    fn accepts_minimal_valid_config() {
        let cfg = parse(
            r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[forbidden_imports]]
in_layers = ["app"]
patterns = ["axum*"]
rule = "APP_NO_AXUM"
"#,
        );
        validate(&cfg).expect("minimal config validates");
    }
}
