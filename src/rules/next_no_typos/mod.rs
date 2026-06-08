//! next-no-typos — flag typo'd Next.js page-export names.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-typos",
    description: "Typo'd page-data export names silently disable the data-fetching hook.",
    remediation: "Use the exact Next.js export names: `getStaticProps`, `getStaticPaths`, `getServerSideProps`, `getInitialProps`. A typo (e.g. `getStaticPorps`) is silently ignored by Next.js.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-typos"),
    categories: &["next"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
