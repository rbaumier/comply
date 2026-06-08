//! drizzle-consistent-table-naming

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-consistent-table-naming",
    description: "Table names passed to `pgTable`/`mysqlTable`/`sqliteTable` should be lowercase snake_case plural.",
    remediation: "Rename the first string argument to a lowercase snake_case plural form (e.g. `user` → `users`, `orderItem` → `order_items`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],

    skip_in_test_dir: false,
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
