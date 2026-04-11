//! jsdoc-missing-example — exported functions with JSDoc must include `@example`.
//!
//! `jsdoc-on-exported` ensures the doc block exists; this rule ensures it
//! actually shows the caller HOW to use the function. The coding-standards
//! skill: "JSDoc on every exported function — block description + @example
//! with call AND return". A description without an example forces every
//! reader to imagine the call site.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-missing-example",
    description: "Exported function JSDoc must include an @example block.",
    remediation: "Add an `@example` block under the description showing a real \
                  call AND its return value: `@example\\n  const r = foo(42);\\n  // => 'forty-two'`. \
                  Examples are the fastest way for callers to understand the API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
};pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
