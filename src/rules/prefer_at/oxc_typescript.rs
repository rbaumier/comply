use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Pattern 1: `arr[arr.length - N]`
            AstKind::ComputedMemberExpression(member) => {
                let obj_text = &ctx.source[member.object.span().start as usize..member.object.span().end as usize];

                let Expression::BinaryExpression(bin) = &member.expression else { return };
                if bin.operator != BinaryOperator::Subtraction {
                    return;
                }

                // Left side should be `<receiver>.length`
                let Expression::StaticMemberExpression(left_member) = &bin.left else { return };
                if left_member.property.name.as_str() != "length" {
                    return;
                }

                let left_obj_text = &ctx.source[left_member.object.span().start as usize..left_member.object.span().end as usize];
                if obj_text != left_obj_text || obj_text.is_empty() {
                    return;
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `.at(…)` over `[….length - index]`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // Pattern 2: `.charAt(…)`
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else { return };
                if member.property.name.as_str() != "charAt" {
                    return;
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `String#at(…)` over `String#charAt(…)`.".into(),
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
    fn flags_length_minus_bracket_access() {
        let d = run_on("const last = arr[arr.length - 1];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(".at("));
    }


    #[test]
    fn flags_char_at() {
        let d = run_on("const c = str.charAt(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("at("));
    }


    #[test]
    fn allows_at() {
        assert!(run_on("const last = arr.at(-1);").is_empty());
    }


    #[test]
    fn allows_normal_bracket_access() {
        assert!(run_on("const first = arr[0];").is_empty());
    }


    #[test]
    fn flags_nested_receiver() {
        let d = run_on("const x = foo.bar[foo.bar.length - 2];");
        assert_eq!(d.len(), 1);
    }
}
