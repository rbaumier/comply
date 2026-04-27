//! sql-no-for-update-without-skip-locked

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-for-update-without-skip-locked",
    description: "`SELECT ... FOR UPDATE` without `SKIP LOCKED` or `NOWAIT` blocks every concurrent worker behind one slow transaction.",
    remediation: "For job-queue / work-stealing patterns use `FOR UPDATE SKIP LOCKED`. For fail-fast contention use `FOR UPDATE NOWAIT`. Plain `FOR UPDATE` is rarely what you want in concurrent code.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "concurrency"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
