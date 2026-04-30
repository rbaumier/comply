//! no-mutation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutation",
    description: "Disallow mutating properties of a `const`-bound value — assignment to its fields still mutates shared state.",
    remediation: "Build a new object/array with the change (spread or structural copy) and assign it to a new binding, or lift the change up to the producer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
