mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-plugin-import-path",
    description: "Importing from `better-auth/plugins` barrel prevents tree-shaking.",
    remediation: "Import from the plugin's specific path: `better-auth/plugins/two-factor`.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/plugins"),
    categories: &["better-auth", "imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
