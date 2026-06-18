mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-sql-raw-with-variable",
    description: "`sql.raw()` with a non-literal argument is a SQL injection vector.",
    remediation: "Use `sql` tagged template literals with parameterized interpolation, or `sql.identifier()` for identifiers.",
    severity: Severity::Error,
    doc_url: Some("https://orm.drizzle.team/docs/sql#sqlraw"),
    categories: &["drizzle", "security"],

    skip_in_test_dir: false,
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
