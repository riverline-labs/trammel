// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.

use std::path::Path;
use trammel::config::{self, Position};

#[test]
fn parses_full_fixture() {
    let cfg =
        config::load(Path::new("tests/fixtures/config/full.toml")).expect("full.toml should parse");

    assert_eq!(cfg.src_root, "src");

    // Layers — names + flags
    let names: Vec<&str> = cfg.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "app",
            "system",
            "transports_web",
            "transports_cli",
            "db",
            "tests"
        ]
    );
    let tests_layer = cfg.layers.iter().find(|l| l.name == "tests").unwrap();
    assert!(tests_layer.implicit_test_context);
    let system_layer = cfg.layers.iter().find(|l| l.name == "system").unwrap();
    assert_eq!(system_layer.exempt_files, vec!["system/connectors/db.rs"]);

    // Each rule kind has at least one entry parsed correctly.
    assert_eq!(cfg.forbidden_imports.len(), 1);
    assert_eq!(cfg.forbidden_imports[0].rule, "APP_BOUNDARY");
    assert_eq!(cfg.forbidden_imports[0].in_layers, vec!["app"]);

    assert_eq!(cfg.forbidden_inline_paths.len(), 2);
    assert_eq!(cfg.forbidden_inline_paths[0].position, Position::Any);
    assert_eq!(cfg.forbidden_inline_paths[1].position, Position::Expr);
    assert_eq!(
        cfg.forbidden_inline_paths[1].in_files,
        vec!["src/transports/web/router.rs"]
    );

    assert_eq!(cfg.forbidden_macros.len(), 1);
    assert!(cfg.forbidden_macros[0].allow_in_test_context);
    assert_eq!(
        cfg.forbidden_macros[0].bare_names_in_layers,
        vec!["app", "system"]
    );

    assert_eq!(cfg.forbidden_methods.len(), 1);
    assert_eq!(cfg.forbidden_methods[0].methods, vec!["unwrap", "expect"]);

    assert_eq!(cfg.required_struct_attrs.len(), 1);
    let stubs = &cfg.required_struct_attrs[0];
    assert_eq!(stubs.struct_name_pattern, "Stub*");
    assert!(stubs.also_apply_to_impls);
    assert_eq!(stubs.required_any_of.len(), 2);

    assert_eq!(cfg.fs_must_exist.len(), 1);
    assert_eq!(cfg.fs_must_exist[0].path, "src/lib.rs");

    assert_eq!(cfg.fs_must_not_exist.len(), 1);
    assert_eq!(cfg.fs_must_not_exist[0].paths.len(), 4);

    assert_eq!(cfg.file_content_scan.len(), 2);
    assert_eq!(
        cfg.file_content_scan[0].forbidden_substrings,
        vec!["session.persona"]
    );
    assert!(cfg.file_content_scan[1].exclude_glob.is_some());

    let n = cfg.n_plus_one.expect("n_plus_one block present");
    assert_eq!(n.opt_out_attribute, "allow_n_plus_one");
    assert_eq!(n.layer_assumes_query, vec!["db"]);
    assert!(n.combinators.contains(&"for_each_concurrent".to_string()));
}
