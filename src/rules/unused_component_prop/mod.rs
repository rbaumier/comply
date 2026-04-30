mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "unused-component-prop",
    description: "React prop declared in the Props type but never read in the component.",
    remediation: "Remove the unused prop from the type definition, or start using \
                  it in the component. Unused props bloat the public API and mislead \
                  consumers into passing data that is silently ignored.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
