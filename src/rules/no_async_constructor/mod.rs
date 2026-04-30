//! no-async-constructor

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-async-constructor",
    description: "Constructors cannot be `async` — they must return the instance, not a Promise.",
    remediation: "Use a static async factory method instead: `static async create() { ... return new MyClass(); }`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
