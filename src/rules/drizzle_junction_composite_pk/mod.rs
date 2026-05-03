//! drizzle-junction-composite-pk

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-junction-composite-pk",
    description: "Junction tables with 2 FK columns must declare a composite `primaryKey`.",
    remediation: "Add `primaryKey({ columns: [t.aId, t.bId] })` in the table options callback so the junction table has a real composite primary key.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
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
