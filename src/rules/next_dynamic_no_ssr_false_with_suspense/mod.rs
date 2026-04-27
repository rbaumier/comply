//! next-dynamic-no-ssr-false-with-suspense

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-dynamic-no-ssr-false-with-suspense",
    description: "`dynamic(..., { ssr: false })` opts the whole subtree out of SSR.",
    remediation: "Wrap the lazy boundary in `<Suspense>` and drop `ssr: false`, or move the import into a client component.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/optimizing/lazy-loading"),
    categories: &["nextjs", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
