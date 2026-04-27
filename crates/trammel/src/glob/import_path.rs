// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Import-path glob matcher.
//!
//! `*` matches any sequence of characters, including `::` separators.
//! A pattern with no `*` is exact-match. Patterns may have `*` at the
//! start, end, middle, or both ends.

/// Returns `true` if `path` matches `pattern` under import-path glob rules.
pub fn matches(pattern: &str, path: &str) -> bool {
    let chunks: Vec<&str> = pattern.split('*').collect();

    if chunks.len() == 1 {
        return path == pattern;
    }

    let first = chunks[0];
    let last = chunks[chunks.len() - 1];

    if !path.starts_with(first) {
        return false;
    }
    if !path.ends_with(last) {
        return false;
    }

    let mut cursor = first.len();
    let end = path.len() - last.len();
    if cursor > end {
        return false;
    }
    for chunk in &chunks[1..chunks.len() - 1] {
        match path[cursor..end].find(chunk) {
            Some(idx) => cursor += idx + chunk.len(),
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn matches_examples_from_spec() {
        assert!(matches("db", "db"));
        assert!(!matches("db", "db::User"));
        assert!(matches("db::*", "db::User"));
        assert!(matches("db::*", "db::queries::find"));
        assert!(matches("axum*", "axum"));
        assert!(matches("axum*", "axum::http"));
        assert!(matches("axum*", "axum::extract::Json"));
        assert!(matches("*transports*", "crate::transports::web"));
        assert!(matches("*transports*", "transports::extractors"));
        assert!(matches("crate::db*", "crate::db"));
        assert!(matches("crate::db*", "crate::db::User"));
        assert!(matches("*::db::*", "crate::db::User"));
        assert!(!matches("axum*", "tower::axum"));
    }
}
