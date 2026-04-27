// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Serde structs for `trammel.toml`. Mirrors §4 of the design.
//!
//! Filesystem assertions (`fs_must_exist`, `fs_must_not_exist`) take
//! paths relative to the **project root**, not `src_root`.

use serde::Deserialize;

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_src_root")]
    pub src_root: String,

    #[serde(default)]
    pub layers: Vec<Layer>,

    #[serde(default)]
    pub forbidden_imports: Vec<ForbiddenImports>,

    #[serde(default)]
    pub forbidden_inline_paths: Vec<ForbiddenInlinePaths>,

    #[serde(default)]
    pub forbidden_macros: Vec<ForbiddenMacros>,

    #[serde(default)]
    pub forbidden_methods: Vec<ForbiddenMethods>,

    #[serde(default)]
    pub required_struct_attrs: Vec<RequiredStructAttrs>,

    #[serde(default)]
    pub fs_must_exist: Vec<FsMustExist>,

    #[serde(default)]
    pub fs_must_not_exist: Vec<FsMustNotExist>,

    #[serde(default)]
    pub file_content_scan: Vec<FileContentScan>,

    pub n_plus_one: Option<NPlusOne>,
}

fn default_src_root() -> String {
    "src".to_string()
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub exempt_files: Vec<String>,
    #[serde(default)]
    pub implicit_test_context: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenImports {
    #[serde(default)]
    pub in_layers: Vec<String>,
    #[serde(default)]
    pub in_files: Vec<String>,
    pub patterns: Vec<String>,
    pub rule: String,
    pub message: Option<String>,
    #[serde(default)]
    pub allow_in_test_context: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenInlinePaths {
    #[serde(default)]
    pub in_layers: Vec<String>,
    #[serde(default)]
    pub in_files: Vec<String>,
    pub patterns: Vec<String>,
    #[serde(default)]
    pub position: Position,
    pub rule: String,
    pub message: Option<String>,
    #[serde(default)]
    pub allow_in_test_context: bool,
}

#[derive(Deserialize, Debug, Default, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Expr,
    Type,
    #[default]
    Any,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenMacros {
    #[serde(default)]
    pub in_layers: Vec<String>,
    #[serde(default)]
    pub in_files: Vec<String>,
    #[serde(default)]
    pub qualified_names: Vec<String>,
    #[serde(default)]
    pub bare_names: Vec<String>,
    #[serde(default)]
    pub bare_names_in_layers: Vec<String>,
    pub rule: String,
    pub message: Option<String>,
    #[serde(default)]
    pub allow_in_test_context: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenMethods {
    #[serde(default)]
    pub in_layers: Vec<String>,
    #[serde(default)]
    pub in_files: Vec<String>,
    pub methods: Vec<String>,
    pub rule: String,
    pub message: Option<String>,
    #[serde(default)]
    pub allow_in_test_context: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RequiredStructAttrs {
    #[serde(default)]
    pub in_layers: Vec<String>,
    #[serde(default)]
    pub in_files: Vec<String>,
    pub struct_name_pattern: String,
    pub required_any_of: Vec<String>,
    #[serde(default)]
    pub also_apply_to_impls: bool,
    pub rule: String,
    pub message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FsMustExist {
    pub path: String,
    pub rule: String,
    pub message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FsMustNotExist {
    pub path: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    pub rule: String,
    pub message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileContentScan {
    pub glob: String,
    pub exclude_glob: Option<String>,
    pub forbidden_substrings: Vec<String>,
    pub rule: String,
    pub message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct NPlusOne {
    pub in_layers: Vec<String>,
    pub db_path_patterns: Vec<String>,
    pub db_macros: Vec<String>,
    pub combinators: Vec<String>,
    pub opt_out_attribute: String,
    #[serde(default)]
    pub layer_assumes_query: Vec<String>,
    pub rule: String,
    pub message: Option<String>,
}
