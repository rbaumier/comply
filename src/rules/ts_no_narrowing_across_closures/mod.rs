//! ts-no-narrowing-across-closures — flag narrowed variables used in async
//! callbacks without re-capture.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-narrowing-across-closures",
    description: "Narrowed variables lose their refinement inside `setTimeout`/`.then`/event handler callbacks.",
    remediation: "Assign the narrowed value to a `const` inside the current scope, then reference the const from the callback.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
