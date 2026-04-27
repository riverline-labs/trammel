// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Filesystem-path glob matching, backed by the `globset` crate.
//!
//! `*` matches within a path segment, `**` matches across segments,
//! `?` matches one character. Globs are evaluated relative to whatever
//! root the caller normalizes against (typically `src_root` or the
//! project root).

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

/// Compile a slice of glob patterns into a single `GlobSet`.
pub fn build_set(globs: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in globs {
        let glob =
            Glob::new(pattern).with_context(|| format!("invalid filesystem glob: `{pattern}`"))?;
        builder.add(glob);
    }
    builder.build().context("failed to build glob set")
}

/// Returns `true` if `rel_path` matches any glob in `set`.
pub fn matches(set: &GlobSet, rel_path: &str) -> bool {
    set.is_match(rel_path)
}

#[cfg(test)]
mod tests {
    use super::{build_set, matches};

    #[test]
    fn smoke() {
        let set = build_set(&[
            "app/**".to_string(),
            "transports/web/**".to_string(),
            "transports/web/middleware.rs".to_string(),
        ])
        .unwrap();

        assert!(matches(&set, "app/foo.rs"));
        assert!(matches(&set, "app/sub/bar.rs"));
        assert!(matches(&set, "transports/web/router.rs"));
        assert!(matches(&set, "transports/web/middleware.rs"));
        assert!(!matches(&set, "system/foo.rs"));
        assert!(!matches(&set, "transports/cli/cmd.rs"));
    }

    #[test]
    fn invalid_pattern_returns_err() {
        let result = build_set(&["[unterminated".to_string()]);
        assert!(result.is_err());
    }
}
