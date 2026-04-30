//! zod-prefer-stringbool — prefer `z.stringbool()` over `z.coerce.boolean()` (Zod v4).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-stringbool",
    description: "`z.coerce.boolean()` only checks truthiness — any non-empty string \
                  (including `\"false\"`) becomes `true`, which breaks HTML form inputs \
                  and query strings.",
    remediation: "Use `z.stringbool()` (Zod v4) to parse `\"true\"/\"false\"/\"1\"/\"0\"` \
                  robustly, or write an explicit `.transform()` with allowed values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
