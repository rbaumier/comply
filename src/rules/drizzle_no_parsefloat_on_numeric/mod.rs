//! drizzle-no-parsefloat-on-numeric

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-parsefloat-on-numeric",
    description: "Reparsing a Drizzle `numeric`/`decimal` column with `parseFloat`/`Number`/unary `+` \
                  and doing arithmetic reintroduces IEEE-754 rounding on money.",
    remediation: "Keep the string and compute with a decimal library (`new Decimal(order.amount)`) \
                  or do the arithmetic in SQL.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/column-types/pg#numeric"),
    categories: &["drizzle", "database"],

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
