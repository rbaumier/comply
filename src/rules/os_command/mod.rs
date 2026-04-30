//! Detects potential OS command injection vulnerabilities.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "os-command",
    description: "Detects potential OS command injection via exec/spawn with dynamic input.",
    remediation: "Use `execFile`/`spawnSync` with separate arguments array, never interpolate user input into shell commands.",
    severity: Severity::Error,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-2076"),
    categories: &["security", "sonarjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
