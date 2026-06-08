//! regex-no-empty-string-match OXC backend.
//!
//! Flags regex literals passed to `.split()` or `.replace()` whose
//! pattern can match the empty string.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
            if j > 0 && (pbytes[j - 1] == b'*' || pbytes[j - 1] == b'+' || pbytes[j - 1] == b'?')
            {
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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        if !pattern_can_match_empty(pattern) {
            return;
        }
        // Walk up to check if this regex is an argument of .split() or .replace().
        if !is_arg_of_split_or_replace(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "regex-no-empty-string-match".into(),
            message: "Regex can match the empty string in `.split()` or `.replace()` \u{2014} this may cause unexpected results.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_arg_of_split_or_replace<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut cur_id = nodes.parent_id(node.id());
    loop {
        if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
            return false;
        }
        let parent_kind = nodes.kind(cur_id);
        if let AstKind::CallExpression(call) = parent_kind {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let name = member.property.name.as_str();
                return name == "split" || name == "replace";
            }
            return false;
        }
        cur_id = nodes.parent_id(cur_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
