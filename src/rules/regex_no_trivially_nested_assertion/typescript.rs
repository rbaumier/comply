//! regex-no-trivially-nested-assertion TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Returns the offsets (within `pattern`) of lookaround assertions that
/// trivially nest another lookaround of the same kind as their first
/// child expression.
fn find_trivially_nested(pattern: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && let Some(kind) = get_lookaround_kind(bytes, i)
        {
            // Scan inside for the same kind of assertion.
            let content_start = i + kind.len() + 2; // skip `(?` + kind chars
            let mut j = content_start;
            let mut depth = 1;
            while j + 3 < len && depth > 0 {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'(' && bytes[j + 1] == b'?' {
                    if let Some(inner_kind) = get_lookaround_kind(bytes, j)
                        && inner_kind == kind
                    {
                        hits.push(i);
                        break;
                    }
                    depth += 1;
                } else if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

fn get_lookaround_kind(bytes: &[u8], pos: usize) -> Option<&'static str> {
    if pos + 3 > bytes.len() || bytes[pos] != b'(' || bytes[pos + 1] != b'?' {
        return None;
    }
    match bytes[pos + 2] {
        b'=' => Some("="),
        b'!' => Some("!"),
        b'<' if pos + 4 <= bytes.len() => match bytes[pos + 3] {
            b'=' => Some("<="),
            b'!' => Some("<!"),
            _ => None,
        },
        _ => None,
    }
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if find_trivially_nested(pattern).is_empty() {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-trivially-nested-assertion",
        "Trivially nested lookaround assertion \u{2014} merge with parent or simplify.".into(),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_nested_same_lookahead() {
        assert_eq!(run_on(r#"const re = /(?=(?=a)b)/;"#).len(), 1);
    }

    #[test]
    fn allows_different_lookaround_kinds() {
        assert!(run_on(r#"const re = /(?=(?!a)b)/;"#).is_empty());
    }

    #[test]
    fn flags_nested_lookbehind() {
        assert_eq!(run_on(r#"const re = /(?<=(?<=a)b)/;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/(?=(?=a)b)";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
