//! sql-constraint-naming-convention

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-constraint-naming-convention",
    description: "Constraints must follow `{table}_{col}_{suffix}` where suffix is pk|fk|key|chk|exl|idx.",
    remediation: "Name constraints explicitly: `CONSTRAINT user_email_key UNIQUE (email)`, `CONSTRAINT order_user_id_fk FOREIGN KEY ...`. Deterministic names simplify migrations and error messages.",
    severity: Severity::Warning,
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
