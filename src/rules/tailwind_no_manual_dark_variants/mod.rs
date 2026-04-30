//! tailwind-no-manual-dark-variants — forbid pairing `dark:bg-*` /
//! `dark:text-*` with a raw palette color when semantic tokens already
//! carry the dark-mode mapping. Design tokens collapse the two into one
//! class (e.g. `bg-background` instead of `bg-white dark:bg-zinc-900`).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-manual-dark-variants",
    description: "Forbid manual `dark:` color variants paired with raw palette colors.",
    remediation: "Replace `bg-white dark:bg-zinc-900` with a semantic token like `bg-background` that already resolves per theme.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
