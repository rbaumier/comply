//! ts-prefer-interface-extends — prefer `interface X extends A, B` to
//! `type X = A & B` for object-type composition.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-interface-extends",
    description: "Prefer `interface X extends A, B` to `type X = A & B` for object composition.",
    remediation: "Convert the type alias to an interface with `extends`. Interfaces give better error messages, support declaration merging, and compile faster.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
