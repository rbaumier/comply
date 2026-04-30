//! no-async-without-await — flag `async` functions that never `await`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-async-without-await",
    description: "`async` function never uses `await`.",
    remediation: "Either remove the `async` keyword (the function returns a value, \
                  not a Promise of one) or add the `await` that justifies it. An \
                  `async` function that never awaits forces callers to unwrap a \
                  Promise for no reason.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["async"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
