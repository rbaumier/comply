//! zod-no-coerce-on-financial — forbid `z.coerce.*` on money/price/amount/currency fields.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-coerce-on-financial",
    description: "`z.coerce.number()` silently accepts `\"NaN\"`, `\" 1.2 \"`, and \
                  empty strings — catastrophic for money/price/amount/currency fields.",
    remediation: "Parse the input explicitly: `z.string().regex(/^\\d+(\\.\\d{1,2})?$/)\
                  .transform(Number)`, and reject anything else with a clear error.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
