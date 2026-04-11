//! no-os-command

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-os-command",
    description: "Shell command execution (`exec`, `spawn`, `child_process`) is a command-injection vector.",
    remediation: "Avoid shelling out when a library or built-in API exists. If unavoidable, never interpolate user input — use `execFile` with an argument array and validate inputs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
