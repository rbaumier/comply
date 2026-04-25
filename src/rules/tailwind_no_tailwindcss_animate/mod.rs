//! tailwind-no-tailwindcss-animate — forbid `tailwindcss-animate`; prefer
//! `tw-animate-css` which is actively maintained and compatible with
//! Tailwind v4.

mod css;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-tailwindcss-animate",
    description: "Forbid imports from `tailwindcss-animate`; use `tw-animate-css` instead.",
    remediation: "Uninstall `tailwindcss-animate` and install `tw-animate-css`, then replace the import / plugin entry accordingly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    let mut def = crate::register_ts_family!(META, typescript);
    def.backends
        .push((Language::Css, Backend::Text(Box::new(css::Check))));
    def
}
