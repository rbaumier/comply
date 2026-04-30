//! ts-no-redundant-async — an `async` function whose only useful work is
//! `return await expr;` and which has no try/catch can drop the `async` and
//! `await` and just return the inner promise.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-redundant-async",
    description: "`async function f() { return await x; }` is redundant — the wrapper adds no behaviour over `function f() { return x; }`.",
    remediation: "Drop `async` and `await`, or keep them only when you need a try/catch \
                  around the awaited expression.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "async"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
