//! no-unnecessary-array-splice-count oxc backend — flag `.splice(x, arr.length)` etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn span_text<'a>(expr: &Expression, source: &'a str) -> &'a str {
    let span = expr.span();
    &source[span.start as usize..span.end as usize]
}

/// A count argument is redundant only when it can do nothing but clamp
/// `.splice()` to "remove through the end": `Infinity`,
/// `Number.POSITIVE_INFINITY`, or the splice receiver's OWN `.length`. A
/// *different* array's `.length` (e.g. `rowData.splice(i, underRows.length)`)
/// removes a specific element count and must be kept.
fn is_unnecessary_count(count: &Expression, receiver: &Expression, source: &str) -> bool {
    match count {
        Expression::Identifier(id) => id.name.as_str() == "Infinity",
        Expression::StaticMemberExpression(m) => match m.property.name.as_str() {
            "length" => span_text(&m.object, source) == span_text(receiver, source),
            "POSITIVE_INFINITY" => {
                matches!(&m.object, Expression::Identifier(obj) if obj.name.as_str() == "Number")
            }
            _ => false,
        },
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Case-sensitive substring match: "splice" is not a substring of
        // "toSpliced", so both literals are needed to reach either method.
        Some(&["splice", "toSpliced"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property "splice" or "toSpliced".
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if method != "splice" && method != "toSpliced" {
            return;
        }

        // Must have exactly 2 arguments.
        if call.arguments.len() != 2 {
            return;
        }

        let Some(count) = call.arguments[1].as_expression() else { return };
        if is_unnecessary_count(count, &member.object, ctx.source) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "The count argument is unnecessary \u{2014} `.splice(start)` already removes all elements from `start`.".into(),
                severity: Severity::Error,
                span: None,
            });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_own_length_count() {
        assert_eq!(run_on("arr.splice(start, arr.length);").len(), 1);
    }

    #[test]
    fn flags_own_member_length_count() {
        assert_eq!(run_on("this.items.splice(0, this.items.length);").len(), 1);
    }

    #[test]
    fn flags_infinity_count() {
        assert_eq!(run_on("arr.splice(start, Infinity);").len(), 1);
    }

    #[test]
    fn flags_number_positive_infinity_count() {
        assert_eq!(run_on("arr.splice(start, Number.POSITIVE_INFINITY);").len(), 1);
    }

    #[test]
    fn flags_own_length_on_to_spliced() {
        assert_eq!(run_on("arr.toSpliced(0, arr.length);").len(), 1);
    }

    #[test]
    fn allows_other_array_length_count() {
        // Different receiver: removes exactly `underRows.length` elements, not to end.
        assert!(run_on("rowData.splice(row.index + 1, underRows.length);").is_empty());
    }

    #[test]
    fn allows_other_member_length_count() {
        assert!(run_on("a.b.splice(0, a.c.length);").is_empty());
    }

    #[test]
    fn allows_numeric_count() {
        assert!(run_on("arr.splice(0, 2);").is_empty());
    }

    #[test]
    fn allows_positive_infinity_on_other_object() {
        assert!(run_on("arr.splice(0, Foo.POSITIVE_INFINITY);").is_empty());
    }

    #[test]
    fn ignores_three_argument_splice() {
        assert!(run_on("arr.splice(0, arr.length, x);").is_empty());
    }

    #[test]
    fn ignores_splice_without_count() {
        assert!(run_on("arr.splice(0);").is_empty());
    }
}
