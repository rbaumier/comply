//! Diagnostic model — unified representation of a single lint violation.
//!
//! Every source (oxlint, clippy, custom rules) converts its findings into
//! this struct so the output formatter can treat them uniformly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single lint violation with location, rule, and remediation message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub rule_id: String,
    pub message: String,
    pub severity: Severity,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}
