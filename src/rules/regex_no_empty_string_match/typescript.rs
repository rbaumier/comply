//! regex-no-empty-string-match TypeScript / JavaScript / TSX backend.
//!
//! Flags regex literals passed to `.split()` or `.replace()` whose
//! pattern can match the empty string (has `*`, `?`, or `{0,…}`
//! without being fully anchored `^…$`). Matching empty is a footgun
//! for split/replace.
//!
//! AST-only detection walks up to find the enclosing `call_expression`
//! whose callee is `.split` or `.replace`, so string literals that look
//! like regex inside other contexts aren't flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn pattern_can_match_empty(pattern: &str) -> bool {
    if is_fully_anchored(pattern) {
        return false;
    }
    if pattern.contains('*') {
        return true;
    }
    if pattern.contains("{0,") {
        return true;
    }
    let pbytes = pattern.as_bytes();
    for j in 0..pbytes.len() {
        if pbytes[j] == b'?' {
            if j > 0 && pbytes[j - 1] == b'\\' {
                continue;
            }
            if j > 0 && (pbytes[j - 1] == b'*' || pbytes[j - 1] == b'+' || pbytes[j - 1] == b'?') {
                continue;
            }
            if j + 1 < pbytes.len() && pbytes[j + 1] == b':' {
                continue;
            }
            return true;
        }
    }
    false
}

fn is_fully_anchored(pattern: &str) -> bool {
    pattern.starts_with('^') && pattern.ends_with('$')
}

/// Walk up from the `regex` node to check whether it is an argument of
/// a `.split(...)` or `.replace(...)` call.
fn is_arg_of_split_or_replace(node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cur = *node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression" {
            if let Some(func) = parent.child_by_field_name("function")
                && func.kind() == "member_expression"
                && let Some(prop) = func.child_by_field_name("property")
                && let Ok(name) = prop.utf8_text(source)
            {
                return name == "split" || name == "replace";
            }
            return false;
        }
        cur = parent;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !pattern_can_match_empty(pattern) {
        return;
    }
    if !is_arg_of_split_or_replace(&node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-string-match",
        "Regex can match the empty string in `.split()` or `.replace()` \u{2014} this may cause unexpected results.".into(),
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
    fn flags_split_with_star() {
        assert_eq!(run_on(r#""abc".split(/a*/);"#).len(), 1);
    }

    #[test]
    fn flags_replace_with_optional() {
        assert_eq!(run_on(r#"str.replace(/x?/g, '-');"#).len(), 1);
    }

    #[test]
    fn flags_replace_with_star() {
        assert_eq!(run_on(r#"s.replace(/\s*/g, '');"#).len(), 1);
    }

    #[test]
    fn allows_split_with_plus() {
        assert!(run_on(r#""abc".split(/a+/);"#).is_empty());
    }

    #[test]
    fn allows_replace_with_anchored() {
        assert!(run_on(r#"s.replace(/^x*$/, '-');"#).is_empty());
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
