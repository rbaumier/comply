//! ts-no-assert-never-in-default — `switch` over a discriminated union with a
//! `default: throw new Error(...)` branch silently goes stale when a new
//! variant is added. Use an exhaustive check (`assertNever(x)` /
//! `const _: never = x;`) so the type-checker catches missing branches.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-assert-never-in-default",
    description: "`switch { default: throw }` without an exhaustive `never` check goes stale when union variants are added.",
    remediation: "Replace `default: throw new Error(...)` with `default: return assertNever(x);` \
                  (or `const _exhaustive: never = x;`) so TypeScript flags the missing case at compile time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
