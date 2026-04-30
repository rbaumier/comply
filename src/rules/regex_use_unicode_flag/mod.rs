//! regex-use-unicode-flag

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-use-unicode-flag",
    description: "Unicode property escapes (`\\p{...}` / `\\P{...}`) require the `u` or `v` flag.",
    remediation: "Add the `u` flag to the regex: `/\\p{Letter}/u`. Without it, `\\p` is not interpreted as a Unicode property escape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
