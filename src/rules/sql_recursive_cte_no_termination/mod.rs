//! sql-recursive-cte-no-termination

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-recursive-cte-no-termination",
    description: "`WITH RECURSIVE` without a `CYCLE` clause or depth guard can run forever on cyclic graphs.",
    remediation: "Add a `CYCLE` clause (PostgreSQL 14+) or guard the recursive term with a depth column (`WHERE depth < N`). Cycles in the data are easy to introduce by accident.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
