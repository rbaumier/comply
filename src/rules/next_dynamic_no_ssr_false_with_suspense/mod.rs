//! next-dynamic-no-ssr-false-with-suspense

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-dynamic-no-ssr-false-with-suspense",
    description: "`dynamic(..., { ssr: false })` opts the whole subtree out of SSR.",
    remediation: "Wrap the lazy boundary in `<Suspense>` and drop `ssr: false`, or move the import into a client component.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/optimizing/lazy-loading"),
    categories: &["nextjs", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
