//! pg-require-limit

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "pg-require-limit",
    description: "SQL `SELECT` statements without a `LIMIT` clause can return unbounded rows.",
    remediation: "Add a `LIMIT n` clause, a `COUNT(..)` / aggregate, or a unique `WHERE` predicate (e.g. `WHERE id = ...`) so the query is bounded.",
    severity: Severity::Error,
    doc_url: Some("https://wiki.postgresql.org/wiki/Don%27t_Do_This#Don.27t_forget_LIMIT"),
    categories: &["database", "postgresql"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

#[cfg(test)]
mod tests {
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    #[test]
    fn meta_skips_test_dir() {
        let file_ctx = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(!super::META.applies_to_file(&file_ctx));
    }
}

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
