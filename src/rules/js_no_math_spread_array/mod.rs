//! js-no-math-spread-array — `Math.min(...array)` / `Math.max(...array)`
//! risks a stack overflow on large arrays (engines cap argument counts
//! around ~65k–100k).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-math-spread-array",
    description: "`Math.min(...array)` / `Math.max(...array)` — stack-overflow risk on \
                  large arrays.",
    remediation: "Use a reduce or for-loop: \
                  `array.reduce((a, b) => a < b ? a : b, Infinity)` for min, \
                  or `-Infinity` and `>` for max.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
