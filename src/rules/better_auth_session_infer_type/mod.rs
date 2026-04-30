mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-session-infer-type",
    description: "Derive `Session` from `typeof auth.$Infer.Session` instead of manual declarations.",
    remediation: "Replace the manual `Session` interface/type with `type Session = typeof auth.$Infer.Session`.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/concepts/typescript"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
