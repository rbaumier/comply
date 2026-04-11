//! sql-no-timestamp-without-tz

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-timestamp-without-tz",
    description: "`TIMESTAMP` without timezone — use `TIMESTAMPTZ` to avoid timezone bugs.",
    remediation: "Replace `TIMESTAMP` with `TIMESTAMPTZ` (or `TIMESTAMP WITH TIME ZONE`). Without timezone info, the same instant is interpreted differently depending on the server's `timezone` setting.",
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
