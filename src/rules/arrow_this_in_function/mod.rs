//! arrow-this-in-function — flag `this` inside an arrow function that has
//! no enclosing regular function/method to bind `this`.
//!
//! Arrow functions don't create their own `this`; they inherit it from the
//! surrounding lexical scope. When an arrow function sits at module scope
//! (or only inside other arrows), its `this` is effectively `undefined`
//! (strict) or the global object — almost always a bug.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "arrow-this-in-function",
    description: "`this` inside an arrow function with no enclosing \
                  regular function or method binds to the outer scope \
                  and is likely a bug.",
    remediation: "Arrow functions don't bind their own `this`, use regular \
                  function or ensure nested in function context",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
