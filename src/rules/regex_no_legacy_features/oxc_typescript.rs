//! regex-no-legacy-features OXC backend.
//!
//! Flags uses of legacy `RegExp` static properties (`RegExp.$1`-`$9`,
//! `RegExp.lastMatch`, etc.) via AST member access / subscript detection.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const LEGACY_PROPS: &[&str] = &[
    "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9",
    "lastMatch", "lastParen", "leftContext", "rightContext", "input",
    "$_", "$&", "$+", "$`", "$'",
];

fn is_regexp_object(expr: &Expression<'_>, source: &str) -> bool {
    let text = &source[expr.span().start as usize..expr.span().end as usize];
    text == "RegExp"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression, AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                if !is_regexp_object(&member.object, ctx.source) {
                    return;
                }
                let prop = member.property.name.as_str();
                if !LEGACY_PROPS.contains(&prop) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Avoid legacy RegExp static properties \u{2014} use capturing groups and match results instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::ComputedMemberExpression(member) => {
                if !is_regexp_object(&member.object, ctx.source) {
                    return;
                }
                let Expression::StringLiteral(s) = &member.expression else { return };
                let prop = s.value.as_str();
                if !LEGACY_PROPS.contains(&prop) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Avoid legacy RegExp static properties \u{2014} use capturing groups and match results instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_regexp_dollar1() {
        assert_eq!(run_on(r#"const x = RegExp.$1;"#).len(), 1);
    }

    #[test]
    fn flags_regexp_lastmatch() {
        assert_eq!(run_on(r#"const x = RegExp.lastMatch;"#).len(), 1);
    }

    #[test]
    fn flags_regexp_subscript() {
        assert_eq!(run_on(r#"const x = RegExp["$&"];"#).len(), 1);
    }

    #[test]
    fn allows_normal_regexp_usage() {
        assert!(run_on(r#"const re = new RegExp("foo");"#).is_empty());
    }

    #[test]
    fn ignores_string_containing_regexp_legacy_syntax() {
        let src = r#"const doc = "Use RegExp.$1 for legacy match";"#;
        assert!(run_on(src).is_empty());
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
