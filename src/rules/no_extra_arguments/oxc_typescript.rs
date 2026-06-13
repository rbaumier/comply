//! no-extra-arguments OXC backend — flag calls with more args than params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, FormalParameters, TSType};
use std::collections::HashMap;
use std::sync::Arc;

struct FunctionInfo {
    param_count: usize,
    has_rest: bool,
}

fn count_params(params: &FormalParameters) -> (usize, bool) {
    let has_rest = params.rest.is_some();
    let count = params.items.len();
    (count, has_rest)
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut functions: HashMap<String, FunctionInfo> = HashMap::new();

        // Pass 1: collect function declarations and arrow/function expression assignments.
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Function(func) => {
                    if let Some(id) = &func.id {
                        let name = id.name.as_str().to_string();
                        let (count, has_rest) = count_params(&func.params);
                        functions.insert(name, FunctionInfo { param_count: count, has_rest });
                    }
                }
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::BindingIdentifier(id) = &decl.id else {
                        continue;
                    };
                    // An explicit type annotation governs the call arity, not the
                    // implementation: TypeScript lets an implementation function
                    // declare fewer parameters than its declared type. Counting the
                    // impl's params (e.g. `() => {}`) would flag every call that
                    // passes the type-required arguments.
                    let params = if let Some(annotation) = &decl.type_annotation {
                        // Inline function type → its params define the arity.
                        // Anything else (a type-alias reference, etc.) is not
                        // cheaply resolvable here, so skip arity checking.
                        let TSType::TSFunctionType(fn_type) = &annotation.type_annotation else {
                            continue;
                        };
                        &fn_type.params
                    } else {
                        let Some(init) = &decl.init else { continue };
                        match init {
                            Expression::ArrowFunctionExpression(arrow) => &arrow.params,
                            Expression::FunctionExpression(func) => &func.params,
                            _ => continue,
                        }
                    };
                    let (count, has_rest) = count_params(params);
                    functions.insert(
                        id.name.as_str().to_string(),
                        FunctionInfo { param_count: count, has_rest },
                    );
                }
                _ => {}
            }
        }

        // Pass 2: check call expressions.
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let name = callee.name.as_str();
            let Some(info) = functions.get(name) else {
                continue;
            };
            if info.has_rest {
                continue;
            }
            let arg_count = call.arguments.len();
            if arg_count > info.param_count {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{name}` expects {} argument(s) but got {arg_count}.",
                        info.param_count
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_extra_argument_on_unannotated_function() {
        let src = r#"
            function foo(a, b) {}
            foo(1, 2, 3);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_extra_argument_on_unannotated_arrow() {
        let src = r#"
            const bar = (x) => x * 2;
            bar(1, 2);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_arity_when_typed_by_type_alias() {
        // The declared type may require more params than the implementation
        // declares; TS allows an impl with fewer params. Flagging based on the
        // impl's param count is the false positive from #1927.
        let src = r#"
            type ExpectType = <T>(value: T) => void
            const expectType: ExpectType = () => {}
            expectType<number>(false)
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn uses_inline_function_type_arity() {
        let src = r#"
            const fn: (a: number) => void = () => {}
            fn(1, 2)
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
