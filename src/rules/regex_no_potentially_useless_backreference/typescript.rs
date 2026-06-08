//! regex-no-potentially-useless-backreference TypeScript / JavaScript / TSX
//! backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Returns `true` when the regex `pattern` contains a backreference in a
/// different top-level alternative than the capturing group it references,
/// e.g. `(a)|\1`.
fn has_cross_alt_backref(pattern: &str) -> bool {
    let alts = split_top_level(pattern);
    if alts.len() < 2 {
        return false;
    }
    for (i, alt) in alts.iter().enumerate() {
        let bytes = alt.as_bytes();
        let mut k = 0;
        while k + 1 < bytes.len() {
            if bytes[k] == b'\\' && bytes[k + 1].is_ascii_digit() && bytes[k + 1] != b'0' {
                let group_num = (bytes[k + 1] - b'0') as usize;
                let mut group_count = 0;
                let mut found_in_other = false;
                for (j, other_alt) in alts.iter().enumerate() {
                    for &b in other_alt.as_bytes() {
                        if b == b'(' {
                            group_count += 1;
                            if group_count == group_num && j != i {
                                found_in_other = true;
                            }
                        }
                    }
                }
                if found_in_other {
                    return true;
                }
            }
            k += 1;
        }
    }
    false
}

/// Split a regex pattern on top-level `|` alternations, ignoring `|`
/// inside groups `(...)` and character classes `[...]`.
fn split_top_level(pattern: &str) -> Vec<&str> {
    let mut alts = Vec::new();
    let bytes = pattern.as_bytes();
    let mut depth = 0;
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\\' => {}
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b'|' if depth == 0 => {
                alts.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alts.push(&pattern[start..]);
    alts
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_cross_alt_backref(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-potentially-useless-backreference",
        "Backreference may be useless \u{2014} some paths do not go through the referenced group.".into(),
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
    fn flags_cross_alt_backref() {
        assert_eq!(run_on(r#"const re = /(a)|\1/;"#).len(), 1);
    }

    #[test]
    fn allows_same_alt_backref() {
        assert!(run_on(r#"const re = /(a)\1/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import {} from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
