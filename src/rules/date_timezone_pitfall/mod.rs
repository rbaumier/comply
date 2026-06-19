//! date-timezone-pitfall — flag date-only `new Date(...)` and `toISOString()` truncation.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "date-timezone-pitfall",
    description: "Flag timezone-shifting date handling: a date-only `new Date(\"YYYY-MM-DD\")` \
                  string (parsed as UTC midnight) and a `toISOString()` result truncated to its \
                  date part (converts to UTC first). Both silently shift the calendar day in \
                  non-UTC zones.",
    remediation: "Construct the date with explicit local components — `new Date(2026, 0, 15)` — \
                  or format with a timezone-aware API (`Intl.DateTimeFormat`, `date-fns-tz`). \
                  To keep a calendar day in local time, read `getFullYear()`/`getMonth()`/\
                  `getDate()` instead of slicing `toISOString()`.",
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
