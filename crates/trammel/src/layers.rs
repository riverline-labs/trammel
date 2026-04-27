// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Path → layer classification.
//!
//! Compiles each layer's `paths` and `exempt_files` globs into a [`GlobSet`]
//! once, then classifies files by first-match on declaration order.

use anyhow::{Context, Result};
use globset::GlobSet;

use crate::config::{Config, Layer};
use crate::glob::fs_path;

pub struct CompiledLayer<'a> {
    pub layer: &'a Layer,
    pub paths: GlobSet,
    pub exempt: GlobSet,
}

pub struct LayerSet<'a> {
    pub layers: Vec<CompiledLayer<'a>>,
}

impl<'a> LayerSet<'a> {
    /// Compile every layer's glob sets up front.
    pub fn build(cfg: &'a Config) -> Result<Self> {
        let mut layers = Vec::with_capacity(cfg.layers.len());
        for layer in &cfg.layers {
            let paths = fs_path::build_set(&layer.paths)
                .with_context(|| format!("layer `{}` has invalid `paths`", layer.name))?;
            let exempt = fs_path::build_set(&layer.exempt_files)
                .with_context(|| format!("layer `{}` has invalid `exempt_files`", layer.name))?;
            layers.push(CompiledLayer {
                layer,
                paths,
                exempt,
            });
        }
        Ok(Self { layers })
    }

    /// First layer in declaration order whose `paths` matches `rel_path`.
    pub fn classify(&self, rel_path: &str) -> Option<&'a Layer> {
        self.layers
            .iter()
            .find(|c| c.paths.is_match(rel_path))
            .map(|c| c.layer)
    }

    /// Is `rel_path` listed under `exempt_files` for `layer`?
    pub fn is_exempt(&self, layer: &Layer, rel_path: &str) -> bool {
        self.layers
            .iter()
            .find(|c| std::ptr::eq(c.layer, layer))
            .map(|c| c.exempt.is_match(rel_path))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        toml::from_str(
            r#"
[[layers]]
name = "app"
paths = ["app/**"]

[[layers]]
name = "system"
paths = ["system/**"]
exempt_files = ["system/connectors/db.rs"]

[[layers]]
name = "transports_web"
paths = ["transports/web/**"]
exempt_files = ["transports/web/middleware.rs"]

[[layers]]
name = "tests"
paths = ["tests.rs", "tests/**"]
implicit_test_context = true
"#,
        )
        .unwrap()
    }

    #[test]
    fn classify_basic() {
        let cfg = cfg();
        let set = LayerSet::build(&cfg).unwrap();

        assert_eq!(set.classify("app/foo.rs").unwrap().name, "app");
        assert_eq!(
            set.classify("system/connectors/db.rs").unwrap().name,
            "system"
        );
        assert_eq!(
            set.classify("transports/web/middleware.rs").unwrap().name,
            "transports_web"
        );
        assert_eq!(set.classify("tests/foo.rs").unwrap().name, "tests");
        assert!(set.classify("something/random.rs").is_none());
    }

    #[test]
    fn implicit_test_context_flag() {
        let cfg = cfg();
        let set = LayerSet::build(&cfg).unwrap();
        let tests = set.classify("tests/foo.rs").unwrap();
        assert!(tests.implicit_test_context);
        let app = set.classify("app/foo.rs").unwrap();
        assert!(!app.implicit_test_context);
    }

    #[test]
    fn exempt_files() {
        let cfg = cfg();
        let set = LayerSet::build(&cfg).unwrap();
        let system = set.classify("system/connectors/db.rs").unwrap();
        let web = set.classify("transports/web/middleware.rs").unwrap();
        assert!(set.is_exempt(system, "system/connectors/db.rs"));
        assert!(!set.is_exempt(system, "system/foo.rs"));
        assert!(set.is_exempt(web, "transports/web/middleware.rs"));
        assert!(!set.is_exempt(web, "transports/web/router.rs"));
    }

    #[test]
    fn first_match_wins() {
        let cfg: Config = toml::from_str(
            r#"
[[layers]]
name = "specific"
paths = ["app/special.rs"]

[[layers]]
name = "app"
paths = ["app/**"]
"#,
        )
        .unwrap();
        let set = LayerSet::build(&cfg).unwrap();
        assert_eq!(set.classify("app/special.rs").unwrap().name, "specific");
        assert_eq!(set.classify("app/other.rs").unwrap().name, "app");
    }
}
