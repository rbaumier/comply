//! `comply list` — enumerate every registered rule.
//!
//! Two output modes: human-readable table and JSON array. JSON output
//! lets editors, CI tooling, and documentation generators consume the
//! registry without parsing terminal text.

use anyhow::{Context, Result};

use crate::diagnostic::Severity;
use crate::rules;

/// Print the registry. `json` toggles between pretty and JSON output.
#[must_use]
pub fn run(json: bool) -> Result<()> {
    let rules = rules::all_rule_defs();
    if json {
        print_json(&rules)
    } else {
        print_human(&rules);
        Ok(())
    }
}

fn print_human(rules: &[rules::RuleDef]) {
    println!("comply: {} rules registered\n", rules.len());
    // Sort by id for deterministic output.
    let mut sorted: Vec<_> = rules.iter().map(|r| &r.meta).collect();
    sorted.sort_by_key(|m| m.id);

    for meta in sorted {
        let severity = match meta.severity {
            Severity::Error => "error  ",
            Severity::Warning => "warning",
        };
        println!("  {severity}  {:<40}  {}", meta.id, meta.description);
    }
}

fn print_json(rules: &[rules::RuleDef]) -> Result<()> {
    let mut sorted: Vec<_> = rules.iter().map(|r| &r.meta).collect();
    sorted.sort_by_key(|m| m.id);

    let json_rules: Vec<_> = sorted
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "description": m.description,
                "remediation": m.remediation,
                "severity": match m.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
                "docUrl": m.doc_url,
            })
        })
        .collect();

    let output = serde_json::to_string_pretty(&json_rules)
        .context("failed to serialize rule list as JSON")?;
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_human_mode_does_not_error() {
        assert!(run(false).is_ok());
    }

    #[test]
    fn run_json_mode_does_not_error() {
        assert!(run(true).is_ok());
    }
}
