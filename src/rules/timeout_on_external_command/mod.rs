mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "timeout-on-external-command",
    description: "`Command::new()` without a timeout can hang indefinitely.",
    remediation: "Wrap the command execution with a timeout (e.g. `tokio::time::timeout` or a custom helper).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
