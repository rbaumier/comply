//! Diagnostic model — unified representation of a single lint violation.
//!
//! Every source (oxlint, clippy, custom rules) converts its findings into
//! this struct so the output formatter can treat them uniformly.

use std::path::PathBuf;

/// A single lint violation with location, rule, and remediation message.
#[derive(Debug)]
#[allow(dead_code)] // Constructed by oxlint::run and rule checks (tasks 5-6).
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub rule_id: String,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug)]
#[allow(dead_code)] // Used by Diagnostic constructors (tasks 5-6).
pub enum Severity {
    Error,
    Warning,
}
