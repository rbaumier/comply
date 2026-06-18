//! sql-no-float-for-money

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod text;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::sql_helpers::contains_word;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-float-for-money",
    description: "`FLOAT`/`DOUBLE`/`REAL` near monetary columns — use `NUMERIC` for money.",
    remediation: "Replace `FLOAT`/`DOUBLE PRECISION`/`REAL` with `NUMERIC(precision, scale)` for any column that holds money, prices, or financial amounts. Floating-point arithmetic introduces rounding errors that compound over transactions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
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
///
/// Both the money word and the float type are matched on word boundaries
/// (via `contains_word`), so a money word that is a substring of an
/// unrelated identifier (`payment` in `AbortPaymentEvent`) or a float
/// type that is a substring of an English word (`REAL` in `really`) does
/// not trigger.
pub(super) fn float_type_for_money_line(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    let has_money = MONEY_WORDS.iter().any(|w| contains_word(&lower, w));
    if !has_money {
        return None;
    }
    FLOAT_TYPES
        .iter()
        .find(|t| contains_word(&lower, &t.to_ascii_lowercase()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::float_type_for_money_line;

    #[test]
    fn flags_genuine_money_float_column() {
        assert_eq!(float_type_for_money_line("price REAL NOT NULL"), Some("REAL"));
        assert_eq!(float_type_for_money_line("amount FLOAT"), Some("FLOAT"));
        assert_eq!(
            float_type_for_money_line("balance DOUBLE PRECISION"),
            Some("DOUBLE")
        );
    }

    #[test]
    fn ignores_real_substring_of_really_issue_1492() {
        // `REAL` is a substring of the English word "really" — must not fire,
        // even with a money word ("payment") elsewhere on the line.
        assert_eq!(
            float_type_for_money_line("payment it doesn't really abort fetch requests"),
            None
        );
    }

    #[test]
    fn ignores_money_word_substring_of_identifier_issue_1492() {
        // `payment` is a substring of `AbortPaymentEvent` — must not fire,
        // even with a float type ("REAL") as a whole word elsewhere.
        assert_eq!(
            float_type_for_money_line("AbortPaymentEvent REAL"),
            None
        );
    }
}
