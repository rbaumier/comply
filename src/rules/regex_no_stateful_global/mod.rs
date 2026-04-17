//! regex-no-stateful-global

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-stateful-global",
    description: "Global regex used with `.test()` or `.exec()` is stateful via `lastIndex`.",
    remediation: "Remove the `g` flag if using `.test()` or `.exec()` repeatedly, or create the regex inside the loop. The `g` flag makes `lastIndex` persist across calls, causing alternating true/false results.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
