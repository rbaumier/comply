//! no-extra-arguments OXC backend — flag calls with more args than params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, FormalParameters};
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
                    let Some(init) = &decl.init else { continue };
                    let params = match init {
                        Expression::ArrowFunctionExpression(arrow) => &arrow.params,
                        Expression::FunctionExpression(func) => &func.params,
                        _ => continue,
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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_extra_argument() {
        let src = r#"
            function foo(a, b) {}
            foo(1, 2, 3);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_arrow_function_extra_args() {
        let src = r#"
            const bar = (x) => x * 2;
            bar(1, 2);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_correct_args() {
        let src = r#"
            function foo(a, b) {}
            foo(1, 2);
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_fewer_args() {
        let src = r#"
            function foo(a, b) {}
            foo(1);
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_rest_params() {
        let src = r#"
            function foo(a, ...rest) {}
            foo(1, 2, 3, 4, 5);
        "#;
        let diags = run_on(src);
        if !diags.is_empty() {
            // Debug: print what we got
            for d in &diags {
                eprintln!("Diagnostic: {}", d.message);
            }
        }
        assert!(diags.is_empty(), "Expected no diagnostics for rest params");
    }


    #[test]
    fn allows_unknown_function() {
        let src = "externalFn(1, 2, 3, 4, 5);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_multiple_extra_calls() {
        let src = r#"
            function foo(a) {}
            foo(1, 2);
            foo(1, 2, 3);
        "#;
        assert_eq!(run_on(src).len(), 2);
    }
}
