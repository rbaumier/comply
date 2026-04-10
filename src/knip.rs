//! knip subprocess — dead code, unused exports, unused dependencies.
//!
//! Why this lives in Comply: the coding-standards skill says "dead code
//! removal as hygiene — unused imports, unreachable branches, commented-
//! out code, and unused exports are liabilities". knip
//! (https://knip.dev) is the de-facto TypeScript dead-code detector and
//! catches things tsc and oxlint can't (cross-file unused exports,
//! unreferenced files, unused npm deps).
//!
//! How it works:
//! 1. `is_available()` probes `knip --version`. Cached in a `OnceLock`.
//! 2. `lint_files()` finds the unique set of project roots (the nearest
//!    `package.json` ancestor for each input file) and runs:
//!
//!        knip --reporter json
//!
//!    from inside that root. knip's JSON reporter emits a single object
//!    with `files`, `dependencies`, `unlistedDependencies`, `exports`,
//!    `types`, `enumMembers`, and `classMembers` keys.
//! 3. We surface three categories as Comply diagnostics:
//!    - `files` → "unreferenced file"
//!    - `dependencies` → "unused dep in package.json"
//!    - `exports` → "unused exported symbol"
//!
//!    The other categories are noisier (member-level dead code) and
//!    deferred to a future iteration.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

pub const RULE_ID_FILE: &str = "ts-unreferenced-file";
pub const RULE_ID_DEP: &str = "ts-unused-dep";
pub const RULE_ID_EXPORT: &str = "ts-unused-export";

pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("knip", &["--version"]))
}

#[must_use = "diagnostics from knip must be reported"]
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
    let output = Command::new("knip")
        .args(["--reporter", "json", "--no-progress"])
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to invoke `knip` in {}", root.display()))?;
    if output.stdout.is_empty() {
        return Ok(vec![]);
    }
    let report: KnipReport = serde_json::from_slice(&output.stdout).with_context(|| {
        format!("failed to parse knip JSON report from {}", root.display())
    })?;
    Ok(convert_report(report, root))
}

fn convert_report(report: KnipReport, root: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for file in report.files {
        diagnostics.push(Diagnostic {
            path: root.join(&file),
            line: 1,
            column: 1,
            rule_id: RULE_ID_FILE.into(),
            message: format!(
                "Unreferenced file `{file}` — no other module imports from it. \
                 Either delete it or add the missing import."
            ),
            severity: Severity::Error,
        });
    }

    for dep in report.dependencies {
        diagnostics.push(Diagnostic {
            path: root.join("package.json"),
            line: 1,
            column: 1,
            rule_id: RULE_ID_DEP.into(),
            message: format!(
                "Unused dependency `{dep}` in package.json — every unused dep \
                 widens the supply chain and slows install. Remove it."
            ),
            severity: Severity::Error,
        });
    }

    for (file, exports) in report.exports {
        for export in exports {
            diagnostics.push(Diagnostic {
                path: root.join(&file),
                line: 1,
                column: 1,
                rule_id: RULE_ID_EXPORT.into(),
                message: format!(
                    "Unused exported symbol `{export}` — no other module imports it. \
                     Either delete the export or remove the `export` keyword if the \
                     symbol is only used internally."
                ),
                severity: Severity::Warning,
            });
        }
    }

    diagnostics
}


/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct KnipReport {
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    /// Map of `file path` → list of unused exported symbol names. Knip
    /// emits this in two slightly different shapes across versions; the
    /// modern shape is a HashMap, which we accept here.
    #[serde(default, deserialize_with = "deserialize_exports")]
    exports: HashMap<String, Vec<String>>,
}

/// knip's exports field can be either a HashMap<String, Vec<String>> or a
/// list of `{ filePath, symbols: [...] }` objects depending on the version.
/// We accept both and normalize to the HashMap shape.
fn deserialize_exports<'de, D>(deserializer: D) -> Result<HashMap<String, Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(HashMap::new());
    }
    if let Some(obj) = value.as_object() {
        let mut out = HashMap::new();
        for (file, symbols) in obj {
            let symbols: Vec<String> = symbols
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            out.insert(file.clone(), symbols);
        }
        return Ok(out);
    }
    if let Some(arr) = value.as_array() {
        let mut out = HashMap::new();
        for entry in arr {
            let Some(file) = entry.get("filePath").and_then(|v| v.as_str()) else {
                continue;
            };
            let symbols: Vec<String> = entry
                .get("symbols")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| {
                            s.as_str()
                                .map(String::from)
                                .or_else(|| s.get("name").and_then(|v| v.as_str()).map(String::from))
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.insert(file.to_string(), symbols);
        }
        return Ok(out);
    }
    Err(D::Error::custom("knip exports field has unexpected shape"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_knip_object_exports() {
        let raw = br#"{"files":["src/dead.ts"],"dependencies":["leftover-pkg"],"exports":{"src/util.ts":["unusedFn","UnusedType"]}}"#;
        let report: KnipReport = serde_json::from_slice(raw).unwrap();
        assert_eq!(report.files, vec!["src/dead.ts"]);
        assert_eq!(report.dependencies, vec!["leftover-pkg"]);
        assert_eq!(report.exports.get("src/util.ts").unwrap().len(), 2);
    }

    #[test]
    fn parses_knip_array_exports() {
        let raw = br#"{"files":[],"dependencies":[],"exports":[{"filePath":"src/x.ts","symbols":[{"name":"foo"}]}]}"#;
        let report: KnipReport = serde_json::from_slice(raw).unwrap();
        assert_eq!(report.exports.get("src/x.ts").unwrap(), &vec!["foo".to_string()]);
    }

    #[test]
    fn handles_empty_report() {
        let raw = br#"{}"#;
        let report: KnipReport = serde_json::from_slice(raw).unwrap();
        assert!(report.files.is_empty());
    }
}
