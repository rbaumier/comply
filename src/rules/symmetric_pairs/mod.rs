//! symmetric-pairs

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "symmetric-pairs",
    description: "Exported function has no symmetric counterpart (get/set, add/remove, open/close, start/stop, create/delete).",
    remediation: "Add the missing counterpart or remove the export if the pair is intentionally incomplete.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
