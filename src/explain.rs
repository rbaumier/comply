//! `comply explain <rule-id>` — pretty-print a rule's metadata.
//!
//! Users who get a violation want to understand WHY the rule exists
//! before fixing it. `comply explain no-throw` shows the id, severity,
//! description, and the full remediation message — same text that ends
//! up in diagnostics, but without requiring the user to trigger the
//! violation first.

use anyhow::{bail, Result};

use crate::diagnostic::Severity;
use crate::rules::{self, meta::RuleMeta};

/// Print the metadata for the rule matching `rule_id`. Returns an error
/// if no rule with that id is registered.
pub fn run(rule_id: &str) -> Result<()> {
    let rules = rules::all_rule_defs();
    let Some(meta) = find_rule(&rules, rule_id) else {
        bail!(
            "unknown rule '{rule_id}'. Run `comply list` to see every \
             registered rule."
        );
    };
    println!("{}", format_meta(meta));
    Ok(())
}

/// Look up a rule by its stable id.
fn find_rule<'a>(rules: &'a [rules::RuleDef], rule_id: &str) -> Option<&'a RuleMeta> {
    rules
        .iter()
        .map(|r| &r.meta)
        .find(|m| m.id == rule_id)
}

/// Render a rule's metadata for the terminal.
fn format_meta(meta: &RuleMeta) -> String {
    let severity = match meta.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let mut out = format!("[{}] ({severity})\n\n", meta.id);
    out.push_str("Description:\n");
    out.push_str("  ");
    out.push_str(meta.description);
    out.push_str("\n\nRemediation:\n  ");
    // Reflow the remediation text with a 2-space indent.
    for line in wrap_lines(meta.remediation, 72) {
        out.push_str(&line);
        out.push_str("\n  ");
    }
    if let Some(url) = meta.doc_url {
        out.push_str("\nDocs: ");
        out.push_str(url);
        out.push('\n');
    }
    out
}

/// Soft-wrap a string at word boundaries to a given character count.
fn wrap_lines(text: &str, width_chars: usize) -> Vec<String> { // comply-ignore: explicit-units — `_chars` IS the unit suffix.
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.len() + word.len() + 1 > width_chars && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_registered_rule() {
        let rules = rules::all_rule_defs();
        assert!(find_rule(&rules, "no-throw").is_some());
    }

    #[test]
    fn returns_none_for_unknown_rule() {
        let rules = rules::all_rule_defs();
        assert!(find_rule(&rules, "does-not-exist").is_none());
    }

    #[test]
    fn format_meta_includes_id_and_remediation() {
        let meta = RuleMeta {
            id: "test-rule",
            description: "A test rule.",
            remediation: "Fix the test.",
            severity: Severity::Warning,
            doc_url: None, categories: &[],
        };
        let out = format_meta(&meta);
        assert!(out.contains("[test-rule]"));
        assert!(out.contains("warning"));
        assert!(out.contains("A test rule."));
        assert!(out.contains("Fix the test."));
    }

    #[test]
    fn run_succeeds_for_real_rule() {
        assert!(run("no-throw").is_ok());
    }

    #[test]
    fn run_errors_for_unknown_rule() {
        assert!(run("does-not-exist").is_err());
    }
}
