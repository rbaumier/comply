//! date-timezone-pitfall — flag date-only `new Date(...)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "date-timezone-pitfall",
    description: "Flag a date-only `new Date(\"YYYY-MM-DD\")` string, which the ECMAScript parser \
                  reads as UTC midnight and thus silently shifts the calendar day in non-UTC \
                  zones.",
    remediation: "Construct the date with explicit local components — `new Date(2026, 0, 15)` — \
                  or format with a timezone-aware API (`Intl.DateTimeFormat`, `date-fns-tz`). \
                  To anchor a date-only string to UTC, append a time and zone (`new Date(\
                  \"2026-01-15T00:00:00Z\")`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness"],

    skip_in_test_dir: true,
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
