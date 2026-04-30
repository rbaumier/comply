//! shadcn-no-manual-dark-overrides — flag `dark:bg-*` / `dark:text-*`
//! etc. paired with explicit light-mode colors; shadcn's semantic tokens
//! already theme-switch automatically.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-manual-dark-overrides",
    description: "Manual `dark:` color overrides reintroduce the duplication shadcn tokens eliminate.",
    remediation: "Replace the light/dark pair (e.g. `bg-white dark:bg-gray-900`) with a semantic token like `bg-background`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
