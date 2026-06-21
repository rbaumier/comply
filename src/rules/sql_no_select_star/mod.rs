//! sql-no-select-star

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod text;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-select-star",
    description: "`SELECT *` wastes bandwidth and prevents covering indexes.",
    remediation: "List columns explicitly: `SELECT id, name, email` instead of `SELECT *`. Explicit columns enable index-only scans and make the API contract visible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

/// True if `text` contains a `SELECT *` wildcard (case-insensitive), allowing
/// for one or two spaces between the keyword and the asterisk.
///
/// A real SQL wildcard `*` is never immediately followed by `/`. The `*/`
/// sequence is a block-comment terminator, so prose like `state of the Select */`
/// (the close of a JSDoc comment) is not a `SELECT *` query and is not matched.
pub(super) fn contains_select_star(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    for needle in ["SELECT *", "SELECT  *"] {
        let mut from = 0;
        while let Some(rel) = upper[from..].find(needle) {
            let star_idx = from + rel + needle.len() - 1;
            if bytes.get(star_idx + 1) != Some(&b'/') {
                return true;
            }
            from = star_idx + 1;
        }
    }
    false
}

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

    #[test]
    fn matches_real_wildcards() {
        assert!(super::contains_select_star("SELECT * FROM users"));
        assert!(super::contains_select_star("select *\n"));
        assert!(super::contains_select_star("SELECT *,"));
        assert!(super::contains_select_star("SELECT *)"));
        assert!(super::contains_select_star("SELECT  * FROM t"));
    }

    #[test]
    fn rejects_comment_terminator() {
        assert!(!super::contains_select_star(
            "/** close the popover on date select */"
        ));
        assert!(!super::contains_select_star("interacting with Select */"));
    }

    #[test]
    fn matches_when_real_wildcard_precedes_a_terminator() {
        // The first `SELECT *` is a real wildcard; a later `select */` must not
        // suppress it.
        assert!(super::contains_select_star("SELECT * FROM t /* select */"));
    }
}
