mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-promise-all",
    description: "Sequential `await` on independent async calls creates an unnecessary waterfall.",
    remediation: "Wrap independent calls in `Promise.all([...])` to run them concurrently.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
