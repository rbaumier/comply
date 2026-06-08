//! prefer-reflect-apply oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".apply"])
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

        // Callee must be a member expression with property `apply`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "apply" {
            return;
        }

        // Skip `Reflect.apply(...)`.
        if let Expression::Identifier(obj) = &member.object
            && obj.name.as_str() == "Reflect" {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);

        // Check for `Function.prototype.apply.call(…)` pattern by reading source text.
        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text.contains("Function.prototype.apply.call") {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Prefer `Reflect.apply(fn, thisArg, args)` over `Function.prototype.apply.call(fn, thisArg, args)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_direct_apply() {
        let d = run_on("fn.apply(null, args);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-reflect-apply");
    }


    #[test]
    fn flags_apply_with_this() {
        let d = run_on("foo.bar.apply(this, args);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_reflect_apply() {
        assert!(run_on("Reflect.apply(fn, null, args);").is_empty());
    }


    #[test]
    fn allows_non_apply_method() {
        assert!(run_on("fn.call(null, args);").is_empty());
    }
}
