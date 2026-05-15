//! next-no-script-component-in-head — `<Script>` inside `<Head>`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-script-component-in-head",
    description: "Next.js `<Script>` inside `<Head>` breaks the loading strategy — `<Script>` is meant to be rendered at body level.",
    remediation: "Move the `<Script>` element outside `<Head>` so Next's `strategy` (`afterInteractive`, `lazyOnload`, …) can do its job.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-script-component-in-head"),
    categories: &["next"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check)))],
    }
}
