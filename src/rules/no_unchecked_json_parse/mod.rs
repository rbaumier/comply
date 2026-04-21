mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unchecked-json-parse",
    description: "`JSON.parse()` returns `any` — validate it before use.",
    remediation: "Pipe the result through a Zod schema (`.safeParse` / `.parse`) or a type guard before using it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
