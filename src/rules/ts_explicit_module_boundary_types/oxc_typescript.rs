//! OxcCheck backend for ts-explicit-module-boundary-types.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration, AstType::ExportDefaultDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ExportNamedDeclaration(export) => {
                let Some(ref decl) = export.declaration else { return };
                match decl {
                    Declaration::FunctionDeclaration(func) => {
                        check_function_params(func, ctx, diagnostics);
                    }
                    Declaration::VariableDeclaration(var_decl) => {
                        for declarator in &var_decl.declarations {
                            let Some(ref init) = declarator.init else { continue };
                            match init {
                                Expression::ArrowFunctionExpression(f) => {
                                    check_arrow_params(f, ctx, diagnostics);
                                }
                                Expression::FunctionExpression(f) => {
                                    check_function_params(f, ctx, diagnostics);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            AstKind::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::FunctionDeclaration(func) =
                    &export.declaration
                {
                    check_function_params(func, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}

fn func_name<'a>(func: &'a Function<'a>) -> &'a str {
    func.id
        .as_ref()
        .map(|id| id.name.as_str())
        .unwrap_or("<anonymous>")
}

fn check_function_params(func: &Function, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let name = func_name(func);
    for param in &func.params.items {
        if param.type_annotation.is_none() {
            let param_name = param_pattern_name(&param.pattern);
            let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Exported function '{name}' parameter '{param_name}' \
                     is missing a type annotation."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn check_arrow_params(
    func: &ArrowFunctionExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let name = "<anonymous>";
    for param in &func.params.items {
        if param.type_annotation.is_none() {
            let param_name = param_pattern_name(&param.pattern);
            let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Exported function '{name}' parameter '{param_name}' \
                     is missing a type annotation."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn param_pattern_name<'a>(pattern: &'a BindingPattern<'a>) -> &'a str {
    match pattern {
        BindingPattern::BindingIdentifier(id) => &id.name,
        _ => "<param>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_missing_return_type_to_dedicated_rule() {
        let diags = run_on("export function foo(a: number) { return a; }");
        assert!(
            diags.is_empty(),
            "return types are owned by ts-explicit-function-return-type"
        );
    }


    #[test]
    fn flags_missing_param_type() {
        let diags = run_on("export function foo(a): number { return 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("parameter"));
    }


    #[test]
    fn allows_fully_typed_export() {
        assert!(run_on("export function foo(a: number): number { return a; }").is_empty());
    }


    #[test]
    fn does_not_flag_non_exported_function() {
        assert!(run_on("function helper(a) { return a; }").is_empty());
    }


    #[test]
    fn flags_exported_arrow_without_types() {
        let diags = run_on("export const foo = (a) => a;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("parameter"));
    }


    #[test]
    fn allows_typed_exported_arrow() {
        assert!(run_on("export const foo = (a: number): number => a;").is_empty());
    }
}
