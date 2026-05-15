//! security-detect-bidi-characters — Unicode bidirectional control chars (trojan source).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-bidi-characters",
    description: "Unicode bidirectional override / isolate characters (U+202A..U+202E, U+2066..U+2069) can hide malicious code from readers — the \"trojan source\" attack.",
    remediation: "Delete the bidi control character. If the file genuinely needs bidirectional text (e.g. an RTL UI string), confine those characters to documented translation files away from executable code.",
    severity: Severity::Error,
    doc_url: Some("https://trojansource.codes/"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
        ],
    }
}
