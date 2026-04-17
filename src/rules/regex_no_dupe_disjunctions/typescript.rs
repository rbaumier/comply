//! regex-no-dupe-disjunctions TypeScript / JavaScript / TSX backend.
//!
//! Detects duplicate alternatives in a regex disjunction, e.g.
//! `/a|b|a/`. AST-only: only flags real `regex` literals, not
//! `RegExp("...")` constructor strings and not unrelated string
//! literals containing `|`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_dupe_alternatives(pattern: &str) -> bool {
    let alts = split_top_level_alternatives(pattern);
    if alts.len() < 2 {
        return false;
    }
    for i in 0..alts.len() {
        for j in (i + 1)..alts.len() {
            if alts[i] == alts[j] && !alts[i].is_empty() {
                return true;
            }
        }
    }
    false
}

fn split_top_level_alternatives(pattern: &str) -> Vec<&str> {
    let mut alts = Vec::new();
    let bytes = pattern.as_bytes();
    let mut depth = 0;
    let mut start = 0;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
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
        i += 1;
    }
    alts.push(&pattern[start..]);
    alts
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_dupe_alternatives(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-dupe-disjunctions",
        "Duplicate alternative in regex disjunction \u{2014} remove the redundant branch.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_alternative() {
        assert_eq!(run_on(r#"const re = /foo|bar|foo/;"#).len(), 1);
    }

    #[test]
    fn allows_unique_alternatives() {
        assert!(run_on(r#"const re = /foo|bar|baz/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
