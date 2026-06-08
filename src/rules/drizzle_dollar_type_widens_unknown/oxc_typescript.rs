//! OXC backend for drizzle-dollar-type-widens-unknown.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::Expression;

pub struct Check;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `obj.$type`.
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        if mem.property.name.as_str() != "$type" {
            return;
        }

        // Must have type arguments.
        let Some(type_params) = &call.type_arguments else {
            return;
        };

        // Extract type argument text from source via span.
        let tp_start = type_params.span.start as usize;
        let tp_end = type_params.span.end as usize;
        if tp_end > ctx.source.len() {
            return;
        }
        let text = &ctx.source[tp_start..tp_end];
        let inner = text
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim();
        if inner != "unknown" && inner != "any" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.$type<{}>()` widens the column type away — pass a concrete type instead.",
                inner
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_dollar_type_unknown() {
        let src = "const c = json('payload').$type<unknown>();";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_dollar_type_any() {
        let src = "const c = json('payload').$type<any>();";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_dollar_type_concrete() {
        let src = "const c = json('payload').$type<{ a: string }>();";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_dollar_type_named() {
        let src = "const c = json('payload').$type<Payload>();";
        assert!(run(src).is_empty());
    }
}
