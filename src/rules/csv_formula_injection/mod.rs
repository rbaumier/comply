//! Detects dynamic CSV cells written without a formula-escape (OWASP CSV
//! injection): a cell whose value starts with `=`, `+`, `-`, or `@` is run as a
//! formula by Excel/Google Sheets when the generated CSV is opened.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "csv-formula-injection",
    description: "Flags a dynamic CSV cell built without a formula-escape (OWASP CSV/formula injection).",
    remediation: "Wrap dynamic cells in a formula-escape helper (e.g. `escapeCsv`) that neutralizes a leading `=`, `+`, `-`, or `@` before joining the row.",
    severity: Severity::Warning,
    doc_url: Some("https://owasp.org/www-community/attacks/CSV_Injection"),
    categories: &["security"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
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
