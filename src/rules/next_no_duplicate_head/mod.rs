//! next-no-duplicate-head — multiple `<Head>` in the same page.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-duplicate-head",
    description: "Multiple `<Head>` elements in the same Next.js page interleave non-deterministically.",
    remediation: "Use a single `<Head>` per page. Compose its children via array spread or `<>...</>` if you need to assemble metadata conditionally.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-duplicate-head"),
    categories: &["next"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check)))],
    }
}
