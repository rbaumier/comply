//! tailwind-require-responsive-text — large heading text (`text-4xl`+)
//! without a responsive variant overflows on phones. Require at least one
//! `sm:text-*` / `md:text-*` / `lg:text-*` when the base size is big.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-responsive-text",
    description: "Headings with `text-4xl+` must also declare a responsive size variant.",
    remediation: "Scale the heading down on mobile, e.g. `text-2xl md:text-4xl` instead of just `text-4xl`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
