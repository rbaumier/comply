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
            // `(?` always introduces a group/lookaround (`(?:`, `(?=`, `(?!`,
            // `(?<=`, `(?<!`, `(?<name>`); a quantifier `?` never follows `(`.
            // So a `?` right after `(` is a group prefix, not an optional
            // quantifier.
            if j > 0 && pbytes[j - 1] == b'(' {
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- True positives (genuine optional / nullable patterns). ---

    #[test]
    fn flags_replace_with_optional() {
        assert_eq!(run_on(r#"const r = s.replace(/a?/g, 'x');"#).len(), 1);
    }

    #[test]
    fn flags_split_with_star() {
        assert_eq!(run_on(r#"const r = s.split(/x*/);"#).len(), 1);
    }

    // --- #3775: lookaround group prefixes must not be read as optional. ---

    #[test]
    fn allows_lookahead_group_prefix() {
        assert!(run_on(r#"const grouped = (s) => s.replace(/(\d)(?=(\d\d\d)+(?!\d))/g, '$1,');"#).is_empty());
    }

    #[test]
    fn allows_alternation_with_negative_lookahead() {
        assert!(run_on(r#"const cased = (d) => d.replace(/[A-Z]+(?![a-z])|[A-Z]/g, (m) => m);"#).is_empty());
    }

    #[test]
    fn allows_positive_lookbehind() {
        assert!(run_on(r#"const r = s.replace(/(?<=\d)x/g, '');"#).is_empty());
    }

    #[test]
    fn allows_negative_lookbehind() {
        assert!(run_on(r#"const r = s.replace(/(?<!\d)x/g, '');"#).is_empty());
    }
}
