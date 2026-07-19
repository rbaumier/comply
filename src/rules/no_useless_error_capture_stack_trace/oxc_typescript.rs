//! no-useless-error-capture-stack-trace OXC backend — flag the redundant
//! single-argument `Error.captureStackTrace(this)` in Error subclass
//! constructors. The two-argument `constructorOpt` form trims stack frames and
//! is left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

const BUILTIN_ERRORS: &[&str] = &[
    "Error", "EvalError", "RangeError", "ReferenceError", "SyntaxError",
    "TypeError", "URIError", "AggregateError", "SuppressedError",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["captureStackTrace"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        // Check superclass is a builtin Error.
        let Some(ref super_class) = class.super_class else {
            return;
        };
        let super_name = match super_class {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !BUILTIN_ERRORS.contains(&super_name) {
            return;
        }

        // Find the constructor.
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Constructor {
                continue;
            }
            let Some(ref func_body) = method.value.body else {
                continue;
            };

            // Walk constructor body for `Error.captureStackTrace(this)`.
            for stmt in &func_body.statements {
                let Statement::ExpressionStatement(expr_stmt) = stmt else {
                    continue;
                };
                let Expression::CallExpression(call) = &expr_stmt.expression else {
                    continue;
                };

                // Callee must be `Error.captureStackTrace`.
                let Expression::StaticMemberExpression(callee) = &call.callee else {
                    continue;
                };
                let Expression::Identifier(obj) = &callee.object else {
                    continue;
                };
                if obj.name.as_str() != "Error" {
                    continue;
                }
                if callee.property.name.as_str() != "captureStackTrace" {
                    continue;
                }

                // Only the single-argument form `Error.captureStackTrace(this)`
                // is useless: V8 already captures the stack via `super()`. A
                // second `constructorOpt` argument trims frames above that
                // constructor from the trace, which `super()` does not do — that
                // form is a meaningful customization and is left alone.
                if call.arguments.len() != 1 {
                    continue;
                }

                let first_is_this = matches!(&call.arguments[0], Argument::ThisExpression(_));
                if !first_is_this {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-useless-error-capture-stack-trace".into(),
                    message: "Unnecessary single-argument \
                              `Error.captureStackTrace(this)` call. Built-in \
                              Error subclasses capture the stack trace \
                              automatically via `super()`."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_single_argument_form() {
        let d = run_on(
            "class MyError extends Error {\n\
               constructor(m) { super(m); Error.captureStackTrace(this); }\n\
             }",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-useless-error-capture-stack-trace");
    }

    #[test]
    fn allows_constructor_opt_class_name() {
        // #5022: the two-argument form trims the constructor frame — meaningful.
        assert!(
            run_on(
                "class MyError extends Error {\n\
                   constructor(m) { super(m); Error.captureStackTrace(this, MyError); }\n\
                 }",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_constructor_opt_this_constructor() {
        // #5022: `Error.captureStackTrace(this, this.constructor)` (commander.js).
        assert!(
            run_on(
                "class CommanderError extends Error {\n\
                   constructor(m) { super(m); Error.captureStackTrace(this, this.constructor); }\n\
                 }",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_constructor_opt_new_target() {
        assert!(
            run_on(
                "class MyError extends Error {\n\
                   constructor(m) { super(m); Error.captureStackTrace(this, new.target); }\n\
                 }",
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_spread_arguments() {
        // A spread call can carry a constructorOpt — argument count is unknown.
        assert!(
            run_on(
                "class MyError extends Error {\n\
                   constructor(m) { super(m); Error.captureStackTrace(...args); }\n\
                 }",
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_this_first_argument() {
        assert!(
            run_on(
                "class MyError extends Error {\n\
                   constructor(m) { super(m); Error.captureStackTrace(obj); }\n\
                 }",
            )
            .is_empty()
        );
    }
}
