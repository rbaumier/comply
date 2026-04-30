//! `comply catalog` — auto-generated rule catalog grouped by category.
//!
//! Two output modes: markdown (for docs/README) and JSON (for tooling).
//! Categories are hierarchical — a rule with `["typescript", "react"]`
//! appears under "typescript > react" in the markdown output.

use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::diagnostic::Severity;
use crate::rules;

/// Generate and print the catalog.
pub fn run(should_emit_json: bool) -> Result<()> {
    let rules = rules::all_rule_defs();
    if should_emit_json {
        print_json(&rules)
    } else {
        print_markdown(&rules);
        Ok(())
    }
}

fn category_label(cats: &[&str]) -> String {
    if cats.is_empty() {
        return "uncategorized".to_string();
    }
    cats.join(" > ")
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

fn backend_label(rule: &rules::RuleDef) -> String {
    use crate::rules::backend::Backend;
    let labels: Vec<&str> = rule
        .backends
        .iter()
        .map(|(_, b)| match b {
            Backend::TreeSitter(_) => "AST",
            Backend::Text(_) => "Text",
            Backend::Oxlint { .. } => "Oxlint",
            Backend::Clippy { .. } => "Clippy",
            Backend::Tsc { .. } => "Tsc",
            Backend::Tsgolint { .. } => "Tsgolint",
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    labels.join(", ")
}

fn print_markdown(rules: &[rules::RuleDef]) {
    // Group rules by their full category path.
    let mut by_category: BTreeMap<String, Vec<&rules::RuleDef>> = BTreeMap::new();
    for rule in rules {
        let label = category_label(rule.meta.categories);
        by_category.entry(label).or_default().push(rule);
    }

    // Sort rules within each category by id.
    for group in by_category.values_mut() {
        group.sort_by_key(|r| r.meta.id);
    }

    let total = rules.len();
    println!("# comply rule catalog");
    println!();
    println!("{total} rules across {} categories.", by_category.len());
    println!();

    // Table of contents.
    println!("## Categories");
    println!();
    for (cat, group) in &by_category {
        let anchor = cat
            .replace(' ', "-")
            .replace('>', "")
            .replace("--", "-")
            .to_lowercase();
        println!("- [{cat}](#{anchor}) ({} rules)", group.len());
    }
    println!();

    // Per-category sections.
    for (cat, group) in &by_category {
        println!("## {cat}");
        println!();
        println!("| Rule | Severity | Backend | Description |");
        println!("|------|----------|---------|-------------|");
        for rule in group {
            let id = rule.meta.id;
            let sev = severity_str(rule.meta.severity);
            let backend = backend_label(rule);
            let desc = rule.meta.description;
            println!("| `{id}` | {sev} | {backend} | {desc} |");
        }
        println!();
    }
}

fn print_json(rules: &[rules::RuleDef]) -> Result<()> {
    let json_rules: Vec<_> = rules
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.meta.id,
                "description": r.meta.description,
                "remediation": r.meta.remediation,
                "severity": severity_str(r.meta.severity),
                "categories": r.meta.categories,
                "backend": backend_label(r),
                "docUrl": r.meta.doc_url,
            })
        })
        .collect();

    let output =
        serde_json::to_string_pretty(&json_rules).context("failed to serialize catalog as JSON")?;
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_output_does_not_error() {
        assert!(run(false).is_ok());
    }

    #[test]
    fn json_output_does_not_error() {
        assert!(run(true).is_ok());
    }
}
