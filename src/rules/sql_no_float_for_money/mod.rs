//! sql-no-float-for-money

mod drizzle;
mod rust;
mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
            (
                Language::TypeScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::TreeSitter(Box::new(drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

pub(super) const MONEY_WORDS: &[&str] = &[
    "price", "amount", "cost", "total", "balance", "fee", "tax", "revenue", "salary", "budget",
    "payment", "invoice",
];
pub(super) const FLOAT_TYPES: &[&str] = &["FLOAT", "DOUBLE", "REAL"];

/// If `line` contains both a money-related identifier and a float type,
/// returns the offending float type name. Otherwise `None`.
pub(super) fn float_type_for_money_line(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    let has_money = MONEY_WORDS.iter().any(|w| lower.contains(w));
    if !has_money {
        return None;
    }
    let upper = line.to_ascii_uppercase();
    FLOAT_TYPES.iter().find(|t| upper.contains(*t)).copied()
}
