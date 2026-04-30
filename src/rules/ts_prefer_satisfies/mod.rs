mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-satisfies",
    description: "`as Type` on object/array literal widens the type — use `satisfies` instead.",
    remediation: "Replace `{...} as Type` with `{...} satisfies Type`. `satisfies` validates the literal without losing the narrow inferred type.",
    severity: Severity::Warning,
    doc_url: Some("https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-9.html"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
