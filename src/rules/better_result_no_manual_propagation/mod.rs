mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-no-manual-propagation",
    description: "Forbid manual error propagation via `if (r.isErr()) return Result.err(r.error)`.",
    remediation: "Use Result.gen + yield* to propagate errors.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
