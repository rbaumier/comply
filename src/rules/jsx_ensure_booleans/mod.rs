//! jsx-ensure-booleans — require the left-hand side of `{x && <Y />}` to be a
//! boolean expression.
//!
//! React renders `0`, `""`, and `NaN` as text, so `{count && <List />}` with
//! `count === 0` visibly renders "0". This rule enforces that conditional JSX
//! rendering uses an explicit boolean: comparisons, `!x`, `!!x`, a boolean
//! literal, or an identifier whose name reads as a boolean
//! (`is…`, `has…`, `should…`, …).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsx-ensure-booleans",
    description: "Left-hand side of `{x && <Jsx />}` must be an unambiguous boolean.",
    remediation: "Coerce with `!!x`, compare (`x > 0`, `x != null`), or use a ternary `cond ? <Jsx /> : null`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
