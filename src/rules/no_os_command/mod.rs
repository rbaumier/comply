//! no-os-command

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-os-command",
    description: "Shell command execution (`exec`, `spawn`, `child_process`) is a command-injection vector.",
    remediation: "Avoid shelling out when a library or built-in API exists. If unavoidable, never interpolate user input — use `execFile` with an argument array and validate inputs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
