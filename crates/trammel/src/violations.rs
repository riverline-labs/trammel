// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Violation type and grouped output formatter.

use std::fmt::Write;
use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub file: PathBuf,
    pub line: usize,
    pub rule: String,
    pub message: String,
}

/// Group violations by rule (declaration order of first occurrence) and
/// render an arch-lint-shaped report. Empty input renders the OK footer.
pub fn render(violations: &[Violation]) -> String {
    if violations.is_empty() {
        return "trammel: OK — no violations found.\n".to_string();
    }

    let mut groups: Vec<(&str, Vec<&Violation>)> = Vec::new();
    for v in violations {
        match groups.iter_mut().find(|(rule, _)| *rule == v.rule.as_str()) {
            Some((_, list)) => list.push(v),
            None => groups.push((&v.rule, vec![v])),
        }
    }

    let mut out = String::new();
    for (rule, list) in &groups {
        let _ = writeln!(out, "── {rule} ({} violations) ──", list.len());
        for v in list {
            let _ = writeln!(out, "  {}:{} — {}", v.file.display(), v.line, v.message);
        }
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "trammel: FAILED — {} violations found.",
        violations.len()
    );
    out
}

/// Machine-readable JSON: a single array, one object per violation. Stable
/// keys (`file`, `line`, `rule`, `message`) so downstream tools can pin a
/// schema. Empty input renders `[]`.
pub fn render_json(violations: &[Violation]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(violations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_renders_ok() {
        assert_eq!(render(&[]), "trammel: OK — no violations found.\n");
    }

    #[test]
    fn json_empty_is_array_literal() {
        assert_eq!(render_json(&[]).expect("renders"), "[]");
    }

    #[test]
    fn json_carries_stable_keys() {
        let v = vec![Violation {
            file: PathBuf::from("src/app/foo.rs"),
            line: 14,
            rule: "APP_NO_AXUM".into(),
            message: "imports axum".into(),
        }];
        let json = render_json(&v).expect("renders");
        assert!(json.contains("\"rule\": \"APP_NO_AXUM\""));
        assert!(json.contains("\"line\": 14"));
        assert!(json.contains("\"file\": \"src/app/foo.rs\""));
        assert!(json.contains("\"message\": \"imports axum\""));
    }

    #[test]
    fn groups_by_rule_in_first_occurrence_order() {
        let v = vec![
            Violation {
                file: PathBuf::from("src/app/foo.rs"),
                line: 14,
                rule: "APP_NO_AXUM".into(),
                message: "app/ imports axum".into(),
            },
            Violation {
                file: PathBuf::from("src/transports/web/router.rs"),
                line: 7,
                rule: "ROUTER_NO_APP".into(),
                message: "router references app::".into(),
            },
            Violation {
                file: PathBuf::from("src/app/bar.rs"),
                line: 3,
                rule: "APP_NO_AXUM".into(),
                message: "app/ imports axum".into(),
            },
        ];
        let expected = "\
── APP_NO_AXUM (2 violations) ──
  src/app/foo.rs:14 — app/ imports axum
  src/app/bar.rs:3 — app/ imports axum

── ROUTER_NO_APP (1 violations) ──
  src/transports/web/router.rs:7 — router references app::

trammel: FAILED — 3 violations found.
";
        assert_eq!(render(&v), expected);
    }
}
