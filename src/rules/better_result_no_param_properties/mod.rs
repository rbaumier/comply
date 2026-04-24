mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-no-param-properties",
    description: "TaggedError constructors must not use parameter properties — call super({ ...args, message }).",
    remediation: "Remove `public`/`private`/`readonly` modifiers on constructor parameters and assign via super().",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
