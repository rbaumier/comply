//! sql-no-varchar

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-varchar",
    description: "`VARCHAR(N)` / `CHAR(N)` — use `TEXT` with a CHECK constraint instead.",
    remediation: "Replace `VARCHAR(N)` with `TEXT` + `CHECK(length(col) <= N)`. VARCHAR's length limit provides no performance benefit in PostgreSQL and silently truncates in some contexts.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
