//! no-misleading-array-reverse OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const MUTATING_METHODS: &[&str] = &["reverse", "sort", "fill", "splice"];

/// Check if a call expression is a mutating array method call (not on a spread copy).
fn is_mutating_call(expr: &Expression, source: &str) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !MUTATING_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    // Allow spread copy patterns like `[...arr].reverse()`
    if let Expression::ArrayExpression(arr) = &member.object {
        let text = &source[arr.span.start as usize..arr.span.end as usize];
        if text.contains("...") {
            return false;
        }
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::ReturnStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".reverse(", ".sort(", ".fill(", ".splice("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    let Some(init) = &declarator.init else {
                        continue;
                    };
                    if is_mutating_call(init, ctx.source) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, init.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Assigning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
            }
            AstKind::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument
                    && is_mutating_call(arg, ctx.source) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, arg.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Returning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
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
    fn flags_const_reverse() {
        assert_eq!(run_on("const reversed = arr.reverse();").len(), 1);
    }


    #[test]
    fn flags_return_sort() {
        assert_eq!(run_on("function f() { return arr.sort(); }").len(), 1);
    }


    #[test]
    fn flags_let_fill() {
        assert_eq!(run_on("let filled = arr.fill(0);").len(), 1);
    }


    #[test]
    fn allows_standalone_call() {
        assert!(run_on("arr.reverse();").is_empty());
    }


    #[test]
    fn allows_spread_copy() {
        assert!(run_on("const reversed = [...arr].reverse();").is_empty());
    }
}
