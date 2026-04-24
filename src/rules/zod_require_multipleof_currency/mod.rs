//! zod-require-multipleof-currency — currency fields require `.multipleOf(0.01)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-multipleof-currency",
    description: "Currency-bearing number schemas that accept arbitrary floats let \
                  through sub-cent precision errors (e.g. `1.2345`), which causes \
                  off-by-penny bugs downstream.",
    remediation: "Constrain to two decimals with `.multipleOf(0.01)` (or use integer \
                  minor units: `.int().nonnegative()` representing cents).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
