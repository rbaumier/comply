//! sql-no-float-for-money

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-float-for-money",
    description: "`FLOAT`/`DOUBLE`/`REAL` near monetary columns — use `NUMERIC` for money.",
    remediation: "Replace `FLOAT`/`DOUBLE PRECISION`/`REAL` with `NUMERIC(precision, scale)` for any column that holds money, prices, or financial amounts. Floating-point arithmetic introduces rounding errors that compound over transactions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
