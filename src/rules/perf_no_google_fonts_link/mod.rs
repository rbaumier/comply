//! perf-no-google-fonts-link — flag `<link href="...fonts.googleapis.com...">`;
//! self-hosting fonts avoids the extra TCP+TLS handshake and third-party
//! privacy baggage.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-no-google-fonts-link",
    description: "Avoid loading fonts from `fonts.googleapis.com`; self-host them instead.",
    remediation: "Download the font files and serve them from your own origin with a `@font-face` declaration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],

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
