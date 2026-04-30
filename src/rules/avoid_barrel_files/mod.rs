mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "avoid-barrel-files",
    description: "Barrel files (pure re-export hubs) hurt tree-shaking and make import graphs opaque.",
    remediation: "Import directly from source modules instead of barrel files",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
