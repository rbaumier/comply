//! perf-no-google-fonts-link — flag `<link href="...fonts.googleapis.com...">`;
//! self-hosting fonts avoids the extra TCP+TLS handshake and third-party
//! privacy baggage.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-no-google-fonts-link",
    description: "Avoid loading fonts from `fonts.googleapis.com`; self-host them instead.",
    remediation: "Download the font files and serve them from your own origin with a `@font-face` declaration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
