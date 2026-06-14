//! no-hardcoded-secret Rust backend — tree-sitter walk that reuses the text
//! backend's per-line secret-shape detection while skipping lines that live
//! inside a `#[cfg(test)]` / `#[test]` context.
//!
//! Inline test modules carry intentional fixture credentials and are not
//! reachable by the directory-based `skip_in_test_dir` lever, so the cfg
//! attribute is detected through the AST instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::no_hardcoded_secret::text;
use crate::rules::rust_helpers::has_test_attribute;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(text::Check::PREFILTER)
    }

    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let test_ranges = collect_test_line_ranges(tree, source);

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line_in_any_range(idx, &test_ranges) || text::is_doc_or_comment_line(line) {
                continue;
            }
            if let Some(kind) = text::scan_line(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-secret".into(),
                    message: format!(
                        "Possible hardcoded secret ({kind}) — move it to an \
                         environment variable or secret store. If this is a \
                         false positive, add a comply-ignore comment explaining."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

/// Inclusive `[start_row, end_row]` line ranges (0-based) covered by a
/// `#[cfg(test)]` / `#[test]` module or function, or by the whole file when it
/// carries a `#![cfg(test)]` inner attribute.
fn collect_test_line_ranges(tree: &tree_sitter::Tree, source: &[u8]) -> Vec<(usize, usize)> {
    let root = tree.root_node();

    // File-level `#![cfg(test)]` marks the entire file as test code.
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "inner_attribute_item"
            && child.utf8_text(source).is_ok_and(|t| t.contains("cfg(test)"))
        {
            return vec![(0, usize::MAX)];
        }
    }

    let mut ranges = Vec::new();
    crate::rules::walker::walk_tree(tree, |node| {
        if (node.kind() == "mod_item" || node.kind() == "function_item")
            && has_test_attribute(node, source)
        {
            ranges.push((node.start_position().row, node.end_position().row));
        }
    });
    ranges
}

fn line_in_any_range(row: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(start, end)| row >= start && row <= end)
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    // Pattern 2 from #1495 — a test-fixture password inside a `#[cfg(test)]`
    // module is intentional test data, not a real credential.
    #[test]
    fn allows_password_fixture_in_cfg_test_module() {
        let src = r#"
#[cfg(test)]
mod tests {
    fn test_encode_special_chars() {
        let password = "x/gfuL?4Zuj{n73m}eeJt1MoreCharsToBeLong";
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_password_fixture_in_test_fn() {
        let src = r#"
#[test]
fn check() {
    let password = "x/gfuL?4Zuj{n73m}eeJt1MoreCharsToBeLong";
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Pattern 1 from #1495 — a bracketed URL-format template placeholder in an
    // error-message string is not a real credential.
    #[test]
    fn allows_bracketed_url_template_placeholder() {
        let src = r#"
fn parse() {
    let msg = "MySQL connection URLs must be in the form `mysql://[[user]:[password]@]host[:port][/database]`";
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative-space guard — a genuine hardcoded credential in non-test,
    // non-placeholder code must still fire.
    #[test]
    fn still_flags_real_password_in_url_outside_tests() {
        let src = r#"
fn connect() {
    let db = "postgres://admin:s3cretProd@db.example.com:5432/prod";
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_real_keyed_literal_outside_tests() {
        let src = r#"const API_KEY: &str = "abcd1234567890abcdef";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // A real credential placed outside the test module must still fire even when
    // the file also contains a `#[cfg(test)]` module.
    #[test]
    fn flags_real_secret_outside_but_skips_inside_test_module() {
        let src = r#"
const API_KEY: &str = "abcd1234567890abcdef";

#[cfg(test)]
mod tests {
    fn check() {
        let password = "x/gfuL?4Zuj{n73m}eeJt1MoreCharsToBeLong";
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
