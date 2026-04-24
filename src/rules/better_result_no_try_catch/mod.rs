mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-no-try-catch",
    description: "Replace try/catch with Result.try({ try, catch }) in better-result modules.",
    remediation: "Wrap the throwing code in Result.try({ try: () => ..., catch: (e) => new TaggedError(...) }).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
