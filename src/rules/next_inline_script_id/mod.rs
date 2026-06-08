//! next-inline-script-id — inline `<Script>` content requires an `id` prop.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-inline-script-id",
    description: "Next.js `<Script>` with inline body (children or `dangerouslySetInnerHTML`) requires an `id` prop so Next can dedupe across re-renders.",
    remediation: "Add a stable `id` prop to the `<Script>` element. Without it Next will re-inject the script on every navigation.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/inline-script-id"),
    categories: &["next"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check)))],
    }
}
