//! next-no-sync-scripts

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-sync-scripts",
    description: "Synchronous `<script>` tags block parsing and hurt LCP.",
    remediation: "Use the `Script` component from `next/script` so Next.js can defer/optimise loading.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/messages/no-sync-scripts"),
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
