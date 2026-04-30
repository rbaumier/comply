//! regex-anchor-precedence TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. Flags patterns where anchors
//! `^` / `$` appear in an alternation without grouping, which binds as
//! `(^a)|(b$)` rather than the usually-intended `^(a|b)$`.
//!
//! AST-only detection eliminates the TextCheck false-positive class
//! where URLs, import paths, and Tailwind arbitrary-value strings were
//! parsed as `/pattern/flags`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Checks if a regex pattern has an anchor precedence issue.
/// Flags `^X|Y` (caret only on first alternative) or `X|Y$` (dollar only on last).
fn has_anchor_precedence_issue(pattern: &str) -> bool {
    let top_level_pipe = find_top_level_pipes(pattern);
    if top_level_pipe.is_empty() {
        return false;
    }

    let mut alternatives = Vec::new();
    let mut prev = 0;
    for &pipe_pos in &top_level_pipe {
        alternatives.push(&pattern[prev..pipe_pos]);
        prev = pipe_pos + 1;
    }
    alternatives.push(&pattern[prev..]);

    if alternatives.len() < 2 {
        return false;
    }

    let first = alternatives[0];
    let last = alternatives[alternatives.len() - 1];

    if first.starts_with('^') {
        let others_have_caret = alternatives[1..].iter().all(|a| a.starts_with('^'));
        if !others_have_caret {
            return true;
        }
    }

    if last.ends_with('$') && !last.ends_with("\\$") {
        let others_have_dollar = alternatives[..alternatives.len() - 1]
            .iter()
            .all(|a| a.ends_with('$') && !a.ends_with("\\$"));
        if !others_have_dollar {
            return true;
        }
    }

    false
}

fn find_top_level_pipes(pattern: &str) -> Vec<usize> {
    let mut pipes = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut depth = 0;
    let mut bracket_depth = 0;
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b'[' => bracket_depth += 1,
            b']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
            }
            b'|' if depth == 0 && bracket_depth == 0 => pipes.push(i),
            _ => {}
        }
        i += 1;
    }
    pipes
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_anchor_precedence_issue(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-anchor-precedence",
        "Anchor in alternation may not bind as expected \u{2014} use `/^(a|b)$/` instead of `/^a|b$/`.".into(),
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
    fn flags_caret_only_on_first() {
        assert_eq!(run_on(r#"const re = /^foo|bar/;"#).len(), 1);
    }

    #[test]
    fn flags_dollar_only_on_last() {
        assert_eq!(run_on(r#"const re = /foo|bar$/;"#).len(), 1);
    }

    #[test]
    fn allows_anchored_group() {
        assert!(run_on(r#"const re = /^(foo|bar)$/;"#).is_empty());
    }

    #[test]
    fn allows_all_anchored() {
        assert!(run_on(r#"const re = /^foo$|^bar$/;"#).is_empty());
    }

    #[test]
    fn allows_no_alternation() {
        assert!(run_on(r#"const re = /^foo$/;"#).is_empty());
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
