//! no-unsafe-shell-exec

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsafe-shell-exec",
    description: "Shell-exec functions should not receive a dynamic command string.",
    remediation: "Use `execFile` / `spawn` with an argv array so arguments aren't re-parsed by the shell. If you need a shell, hard-code the template and pass user input as argv parameters — never interpolate it into the command string.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
