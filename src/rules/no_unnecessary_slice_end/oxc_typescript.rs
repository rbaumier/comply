//! no-unnecessary-slice-end oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when the `end` argument is provably redundant for `receiver.slice(...)`.
///
/// `Infinity` / `Number.POSITIVE_INFINITY` always mean "to the end". A
/// `.length` access is only redundant when it reads the length of the *same*
/// expression being sliced — `arr.slice(0, arr.length)`. `full.slice(0,
/// prefix.length)` slices a different array's length and is meaningful.
fn is_unnecessary_end(end_text: &str, receiver_text: &str) -> bool {
    let trimmed = end_text.trim();
    if trimmed == "Infinity" || trimmed == "Number.POSITIVE_INFINITY" {
        return true;
    }
    trimmed
        .strip_suffix(".length")
        .is_some_and(|base| base.trim() == receiver_text.trim())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["slice"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a member expression with property "slice".
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "slice" {
            return;
        }

        // Must have exactly 2 arguments.
        if call.arguments.len() != 2 {
            return;
        }

        let second = &call.arguments[1];
        let second_text =
            &ctx.source[second.span().start as usize..second.span().end as usize];
        let receiver_text = &ctx.source
            [member.object.span().start as usize..member.object.span().end as usize];

        if is_unnecessary_end(second_text, receiver_text) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "The `end` argument is unnecessary \u{2014} `.slice(start)` already goes to the end.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    fn run(src: &str) -> Vec<Diagnostic> {
        run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_slice_to_own_length() {
        assert_eq!(run("const x = arr.slice(0, arr.length);").len(), 1);
    }

    #[test]
    fn flags_slice_to_infinity() {
        assert_eq!(run("const x = arr.slice(2, Infinity);").len(), 1);
    }

    #[test]
    fn flags_slice_to_own_length_member_receiver() {
        assert_eq!(run("const x = this.items.slice(0, this.items.length);").len(), 1);
    }

    // Regression #594 — `full.slice(0, prefix.length)` slices a *different*
    // array's length; the `end` argument is essential.
    #[test]
    fn no_fp_slice_to_other_array_length_issue_594() {
        assert!(run("const x = full.slice(0, prefix.length);").is_empty());
    }

    #[test]
    fn allows_single_arg_slice() {
        assert!(run("const x = arr.slice(2);").is_empty());
    }



    #[test]
    fn flags_slice_with_length() {
        let d = crate::rules::test_helpers::run_oxc_ts("arr.slice(2, arr.length);", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-slice-end");
    }


    #[test]
    fn flags_slice_with_infinity() {
        let d = crate::rules::test_helpers::run_oxc_ts("str.slice(0, Infinity);", &Check);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_slice_without_end() {
        let d = crate::rules::test_helpers::run_oxc_ts("arr.slice(2);", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn allows_slice_with_numeric_end() {
        let d = crate::rules::test_helpers::run_oxc_ts("arr.slice(2, 5);", &Check);
        assert!(d.is_empty());
    }
}
