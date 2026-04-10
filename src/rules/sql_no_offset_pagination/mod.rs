//! sql-no-offset-pagination

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-offset-pagination",
    description: "`OFFSET` pagination is O(N) on deep pages — use cursor-based (keyset) pagination.",
    remediation: "Replace `LIMIT N OFFSET M` with cursor-based pagination: `WHERE id > :last_id ORDER BY id LIMIT N`. OFFSET scans and discards M rows every time.",
    severity: Severity::Warning,
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
