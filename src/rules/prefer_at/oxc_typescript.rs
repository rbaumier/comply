use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

/// True when the computed member at `member_span` occupies a write position —
/// the left side of an assignment (`arr[arr.length-1] = …`, `… += …`), the
/// operand of an update (`arr[arr.length-1]++`), a for-in/of binding, or a
/// destructuring target. `.at()` returns a value, not a reference, so it can
/// never stand in those positions (`arr.at(-1) = …` is a hard syntax error);
/// only reads should be flagged. The LHS of an assignment is matched by span so
/// the read on the RHS (`x = arr[arr.length-1]`) still flags.
fn is_write_position(parent: AstKind, member_span: Span) -> bool {
    match parent {
        AstKind::UpdateExpression(_)
        | AstKind::ArrayAssignmentTarget(_)
        | AstKind::ObjectAssignmentTarget(_)
        | AstKind::AssignmentTargetWithDefault(_) => true,
        AstKind::AssignmentExpression(assign) => assign.left.span() == member_span,
        AstKind::ForOfStatement(stmt) => stmt.left.span() == member_span,
        AstKind::ForInStatement(stmt) => stmt.left.span() == member_span,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Pattern 1: `arr[arr.length - N]`
            AstKind::ComputedMemberExpression(member) => {
                // `.at()` is not a valid assignment/update target, so a write
                // position (`arr[arr.length-1] = …`, `++`, destructuring) must
                // not be flagged — only reads.
                let parent = semantic.nodes().parent_node(node.id());
                if is_write_position(parent.kind(), member.span) {
                    return;
                }

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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_length_minus_bracket_read() {
        let d = run("const last = arr[arr.length - 1];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(".at("));
    }

    #[test]
    fn flags_read_in_call_and_return() {
        assert_eq!(run("foo(arr[arr.length - 1]);").len(), 1);
        assert_eq!(run("function f() { return arr[arr.length - 1]; }").len(), 1);
    }

    #[test]
    fn flags_char_at() {
        let d = run("const c = str.charAt(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("at("));
    }

    #[test]
    fn allows_at() {
        assert!(run("const last = arr.at(-1);").is_empty());
    }

    #[test]
    fn allows_normal_bracket_access() {
        assert!(run("const first = arr[0];").is_empty());
    }

    #[test]
    fn flags_nested_receiver_read() {
        assert_eq!(run("const x = foo.bar[foo.bar.length - 2];").len(), 1);
    }

    // Regression for #3912: write positions cannot use `.at()` (it returns a
    // value, not a reference), so they must not be flagged.
    #[test]
    fn allows_assignment_target() {
        assert!(run("lines[lines.length - 1] = x;").is_empty());
    }

    #[test]
    fn allows_compound_assignment_target() {
        assert!(run("words[words.length - 1] += \" \" + word;").is_empty());
    }

    #[test]
    fn allows_update_target() {
        assert!(run("arr[arr.length - 1]++;").is_empty());
    }

    #[test]
    fn allows_destructuring_and_for_of_targets() {
        assert!(run("[arr[arr.length - 1]] = [x];").is_empty());
        assert!(run("for (arr[arr.length - 1] of items) {}").is_empty());
    }

    // The RHS of an assignment is a read — it must still flag, even though its
    // parent is also an `AssignmentExpression`.
    #[test]
    fn flags_read_on_assignment_rhs() {
        assert_eq!(run("y = arr[arr.length - 1];").len(), 1);
    }
}
