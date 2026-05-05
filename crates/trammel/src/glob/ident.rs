// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Identifier glob matcher.
//!
//! `*` matches any sequence of characters within an identifier.
//! `**` is rejected as a config error (it has no segment-aware meaning here).
//! Bare names with no `*` are exact-match.

use anyhow::{anyhow, Result};

/// Returns `true` if `ident` matches `pattern` under identifier glob rules.
pub fn matches(pattern: &str, ident: &str) -> bool {
    let chunks: Vec<&str> = pattern.split('*').collect();

    if chunks.len() == 1 {
        return ident == pattern;
    }

    let first = chunks[0];
    let last = chunks[chunks.len() - 1];

    if !ident.starts_with(first) {
        return false;
    }
    if !ident.ends_with(last) {
        return false;
    }

    let mut cursor = first.len();
    let end = ident.len() - last.len();
    if cursor > end {
        return false;
    }
    for chunk in &chunks[1..chunks.len() - 1] {
        match ident[cursor..end].find(chunk) {
            Some(idx) => cursor += idx + chunk.len(),
            None => return false,
        }
    }
    true
}

/// Validates an identifier pattern. Rejects `**`.
pub fn validate(pattern: &str) -> Result<()> {
    if pattern.contains("**") {
        return Err(anyhow!(
            "identifier pattern `{pattern}` contains `**`, which has no meaning in identifier globs"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{matches, validate};

    #[test]
    fn identifier_globs() {
        assert!(matches("Stub*", "Stub"));
        assert!(matches("Stub*", "StubFoo"));
        assert!(matches("Stub*", "Stubby"));
        assert!(!matches("Stub*", "TStub"));
        assert!(matches("*Port", "TenantPort"));
        assert!(!matches("Stub", "StubFoo"));
    }

    #[test]
    fn identifier_globs_prefix_suffix_combinations() {
        // Suffix mismatch even though prefix matches.
        assert!(!matches("Foo*Bar", "FooBaz"));
        // Prefix+suffix overlap such that the wildcard region has zero
        // remaining width — must reject.
        assert!(!matches("ab*ba", "aba"));
        // Multi-chunk pattern: at least one middle chunk must be located.
        assert!(matches("Foo*Bar*Baz", "FooXBarYBaz"));
        assert!(!matches("Foo*Bar*Baz", "FooXBzzYBaz"));
    }

    #[test]
    fn validate_rejects_double_star() {
        assert!(validate("Stub*").is_ok());
        assert!(validate("*Stub").is_ok());
        assert!(validate("Stub").is_ok());
        assert!(validate("**").is_err());
        assert!(validate("Stub**Foo").is_err());
    }
}
