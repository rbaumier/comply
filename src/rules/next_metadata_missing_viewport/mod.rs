//! next-metadata-missing-viewport

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-metadata-missing-viewport",
    description: "Pages exporting `metadata` should also export `viewport`.",
    remediation: "Add `export const viewport: Viewport = { width: 'device-width', initialScale: 1 };` next to your `metadata` export.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/generate-viewport"),
    categories: &["nextjs", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
