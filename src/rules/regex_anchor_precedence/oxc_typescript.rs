//! regex-anchor-precedence OxcCheck backend — flags patterns where anchors
//! `^` / `$` appear in an alternation without grouping.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Checks if a regex pattern has an anchor precedence issue.
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

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        if !has_anchor_precedence_issue(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Anchor in alternation may not bind as expected \u{2014} use `/^(a|b)$/` instead of `/^a|b$/`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
