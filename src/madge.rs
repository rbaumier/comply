//! madge subprocess — circular dependency detection.
//!
//! Why this lives in Comply: circular imports break tree-shaking, slow
//! TypeScript compilation, and make module ordering depend on import
//! sequence at runtime. madge (https://github.com/pahen/madge) walks the
//! ESM/CJS import graph and lists every cycle.
//!
//! How it works:
//! 1. `is_available()` probes `madge --version`. Cached in a `OnceLock`.
//! 2. `lint_files()` finds the unique set of project roots and runs
//!
//!        madge --json --circular <root>
//!
//!    The output is an array of cycle paths; each path is a list of
//!    files traversed to complete the cycle (and back to the start).
//! 3. Each cycle becomes one Comply diagnostic on the FIRST file of the
//!    cycle, listing the cycle in the message.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

pub const RULE_ID: &str = "no-circular-imports";

pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("madge", &["--version"]))
}

#[must_use = "diagnostics from madge must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut diagnostics = Vec::new();
    for root in runner_helpers::collect_unique_roots(files, "package.json") {
        diagnostics.extend(scan_root(&root)?);
    }
    Ok(diagnostics)
}

fn scan_root(root: &Path) -> Result<Vec<Diagnostic>> {
    let output = Command::new("madge")
        .args(["--json", "--circular"])
        .arg(root)
        .output()
        .with_context(|| format!("failed to invoke `madge` in {}", root.display()))?;
    if output.stdout.is_empty() {
        return Ok(vec![]);
    }
    // madge --json --circular emits an array of cycles. Some madge
    // versions wrap that in `{ "circular": [...] }`; we accept both.
    let cycles: Vec<Vec<String>> = parse_cycles(&output.stdout)
        .with_context(|| format!("failed to parse madge output from {}", root.display()))?;
    Ok(convert_cycles(cycles, root))
}

fn parse_cycles(bytes: &[u8]) -> Result<Vec<Vec<String>>> {
    if let Ok(arr) = serde_json::from_slice::<Vec<Vec<String>>>(bytes) {
        return Ok(arr);
    }
    let wrapped: MadgeWrapped = serde_json::from_slice(bytes)?;
    Ok(wrapped.circular)
}

fn convert_cycles(cycles: Vec<Vec<String>>, root: &Path) -> Vec<Diagnostic> {
    cycles
        .into_iter()
        .filter(|c| !c.is_empty())
        .map(|cycle| {
            let first = &cycle[0];
            let chain = cycle.join(" → ");
            Diagnostic {
                path: root.join(first),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "Circular import: {chain} → {first}. Cycles break tree-shaking, \
                     slow TS compilation, and make module ordering depend on import \
                     sequence. Break the cycle by extracting the shared types into \
                     a leaf module."
                ),
                severity: Severity::Error,
                span: None,
            }
        })
        .collect()
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct MadgeWrapped {
    #[serde(default)]
    circular: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_array_of_cycles() {
        let raw = br#"[["a.ts","b.ts"],["c.ts","d.ts","e.ts"]]"#;
        let cycles = parse_cycles(raw).unwrap();
        assert_eq!(cycles.len(), 2);
        assert_eq!(cycles[0], vec!["a.ts", "b.ts"]);
    }

    #[test]
    fn parses_wrapped_object() {
        let raw = br#"{"circular":[["a.ts","b.ts"]]}"#;
        let cycles = parse_cycles(raw).unwrap();
        assert_eq!(cycles.len(), 1);
    }

    #[test]
    fn empty_cycles() {
        let raw = br#"[]"#;
        let cycles = parse_cycles(raw).unwrap();
        assert!(cycles.is_empty());
    }
}
