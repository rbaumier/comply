//! zod-no-transform-in-record-key OXC backend — flag `.transform()` in `z.record()` key schema.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Walk the source text of the first argument looking for `.transform`.
/// We use the source text approach (same as TreeSitter version) since
/// the first argument subtree can be arbitrarily chained.
fn arg_source_contains_transform(source: &str, start: u32, end: u32) -> bool {
    let text = &source[start as usize..end as usize];
    text.contains(".transform")
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

        // Callee must be z.record
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "record" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "z" {
            return;
        }

        // First argument is the key schema
        let Some(first_arg) = call.arguments.first() else { return };
        let span = first_arg.span();

        if !arg_source_contains_transform(ctx.source, span.start, span.end) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.transform()` in a `z.record()` key schema mutates object keys after validation — drop the transform or move it to the value schema.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_transform_in_record_key() {
        assert_eq!(
            run("const r = z.record(z.string().transform(s => s.toLowerCase()), z.number());")
                .len(),
            1
        );
    }


    #[test]
    fn flags_transform_in_record_key_chained() {
        assert_eq!(
            run("const r = z.record(z.string().trim().transform(s => s), valueSchema);").len(),
            1
        );
    }


    #[test]
    fn allows_plain_key_schema() {
        assert!(run("const r = z.record(z.string(), z.number());").is_empty());
    }


    #[test]
    fn allows_transform_in_value_schema() {
        assert!(
            run("const r = z.record(z.string(), z.number().transform(n => n * 2));").is_empty()
        );
    }


    #[test]
    fn ignores_unrelated_record_call() {
        assert!(run("const r = other.record(z.string().transform(s => s), v);").is_empty());
    }
}
