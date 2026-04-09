#![allow(dead_code)] // Called by main orchestrator (task 12).

//! Rule engine — reads source files and runs all applicable custom rules.
//!
//! How it works:
//! 1. Collect all registered rules from rules::all_rules().
//! 2. For each file, read its contents once.
//! 3. Run every rule whose language list includes the file's language.
//! 4. Collect and return all diagnostics.

use anyhow::{Context, Result};
use std::fs;

use crate::diagnostic::Diagnostic;
use crate::files::SourceFile;
use crate::rules;
#[allow(unused_imports)]
use crate::rules::Rule;

/// Run all applicable custom rules on the given files.
pub fn run_custom_rules(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let rules = rules::all_rules();
    let mut diagnostics = Vec::new();

    for file in files {
        let source = fs::read_to_string(&file.path)
            .with_context(|| format!("failed to read {}", file.path.display()))?;

        for rule in &rules {
            if rule.languages().contains(&file.language) {
                diagnostics.extend(rule.check(&file.path, &source, file.language));
            }
        }
    }

    Ok(diagnostics)
}
