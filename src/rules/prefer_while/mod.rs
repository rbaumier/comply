//! prefer-while

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-while",
    description: "`for (;;)` or `for (;cond;)` without init/update — use `while` instead.",
    remediation: "Replace `for (;;)` with `while (true)` and `for (;condition;)` with `while (condition)`. The `for` form hides intent when init and update are unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
