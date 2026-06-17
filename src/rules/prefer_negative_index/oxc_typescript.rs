//! OxcCheck backend for prefer-negative-index.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const METHODS: &[&str] = &["slice", "splice", "toSpliced", "at", "with", "subarray"];

/// Whether argument position `i` of `method` is a negative-indexable index.
///
/// `splice(start, deleteCount, ...items)` and `toSpliced(...)` only take an
/// index at position 0 (`start`); `deleteCount` is a count (a negative value
/// clamps to 0) and trailing args are items. `with(index, value)` only takes
/// an index at position 0. Every position of the other methods is an index.
fn is_index_position(method: &str, i: usize) -> bool {
    match method {
        "splice" | "toSpliced" | "with" => i == 0,
        _ => true,
    }
}

/// Check if an expression is `<receiver>.length - <expr>`.
fn is_length_minus<'a>(expr: &Expression<'a>, source: &str, receiver_text: &str) -> bool {
    let Expression::BinaryExpression(bin) = expr else { return false };
    if bin.operator != BinaryOperator::Subtraction {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &bin.left else { return false };
    if member.property.name.as_str() != "length" {
        return false;
    }
    let obj_span = member.object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    obj_text == receiver_text
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method_name = member.property.name.as_str();
        if !METHODS.contains(&method_name) {
            return;
        }

        let obj_span = member.object.span();
        let receiver = &ctx.source[obj_span.start as usize..obj_span.end as usize];
        if receiver.is_empty() {
            return;
        }

        for (i, arg) in call.arguments.iter().enumerate() {
            if !is_index_position(method_name, i) {
                continue;
            }
            let Some(expr) = arg.as_expression() else { continue };
            if is_length_minus(expr, ctx.source, receiver) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer negative index over `.length - index`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return; // one diagnostic per call
            }
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

    // True positives: index-typed positions must still fire.

    #[test]
    fn flags_slice_length_minus() {
        assert_eq!(run("const x = str.slice(str.length - 3);").len(), 1);
    }

    #[test]
    fn flags_slice_end_length_minus() {
        // `slice(start, end)` — arg 1 is also an index.
        assert_eq!(run("const x = arr.slice(0, arr.length - 1);").len(), 1);
    }

    #[test]
    fn flags_splice_start_length_minus() {
        // `splice(start, deleteCount)` — arg 0 (`start`) is an index.
        assert_eq!(run("arr.splice(arr.length - 1, 1);").len(), 1);
    }

    #[test]
    fn flags_at_length_minus() {
        assert_eq!(run("const last = arr.at(arr.length - 1);").len(), 1);
    }

    #[test]
    fn flags_with_index_length_minus() {
        // `with(index, value)` — arg 0 (`index`) is an index.
        assert_eq!(run("const x = arr.with(arr.length - 1, v);").len(), 1);
    }

    // Regressions (#3970): count/value positions must NOT be flagged.

    #[test]
    fn allows_splice_delete_count_length_minus() {
        // `splice(start, deleteCount)` — arg 1 is a count; negative clamps to 0.
        assert!(run("arr.splice(0, arr.length - 2);").is_empty());
    }

    #[test]
    fn allows_to_spliced_delete_count_length_minus() {
        // `toSpliced(start, deleteCount, ...items)` — arg 1 is a count.
        assert!(run("const x = arr.toSpliced(0, arr.length - 2);").is_empty());
    }

    #[test]
    fn allows_with_value_length_minus() {
        // `with(index, value)` — arg 1 is the value, not an index.
        assert!(run("const x = arr.with(0, arr.length - 1);").is_empty());
    }

    // Existing negative cases.

    #[test]
    fn allows_negative_index() {
        assert!(run("const x = str.slice(-3);").is_empty());
    }

    #[test]
    fn allows_different_receiver() {
        assert!(run("const x = str.slice(other.length - 3);").is_empty());
    }

    #[test]
    fn allows_normal_slice() {
        assert!(run("const x = str.slice(0, 5);").is_empty());
    }
}
