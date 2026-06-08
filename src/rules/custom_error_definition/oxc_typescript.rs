//! custom-error-definition OXC backend — flag `this.name = 'X'` and
//! `this.message = ...` in Error subclass constructors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, Expression, MethodDefinitionKind, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_error_name(name: &str) -> bool {
    name.ends_with("Error") && name.starts_with(|c: char| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
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

        // Must extend something that looks like an Error.
        let Some(super_class) = &class.super_class else {
            return;
        };
        let super_name = match super_class {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !is_error_name(super_name) {
            return;
        }

        // Get class name.
        let Some(class_id) = &class.id else {
            return;
        };
        let class_name = class_id.name.as_str();

        // Find the constructor.
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Constructor {
                continue;
            }
            let Some(func_body) = &method.value.body else {
                continue;
            };

            // Walk statements in the constructor body.
            for stmt in &func_body.statements {
                let Statement::ExpressionStatement(expr_stmt) = stmt else {
                    continue;
                };
                let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                    continue;
                };
                // Get the left-hand side text from source.
                let left_span = assign.left.span();
                let left_text =
                    &ctx.source[left_span.start as usize..left_span.end as usize];

                if left_text == "this.name" {
                    let (line, column) = byte_offset_to_line_col(
                        ctx.source,
                        expr_stmt.span.start as usize,
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Use a class field `name = '{class_name}';` instead \
                             of setting `this.name` in the constructor."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }

                if left_text == "this.message" {
                    let (line, column) = byte_offset_to_line_col(
                        ctx.source,
                        expr_stmt.span.start as usize,
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Pass the error message to `super()` instead \
                                  of setting `this.message`."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
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
    fn flags_this_name_in_constructor() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super(message);
        this.name = 'MyError';
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class field"));
    }


    #[test]
    fn flags_this_message_in_constructor() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super();
        this.message = message;
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("super()"));
    }


    #[test]
    fn flags_both_name_and_message() {
        let code = r#"
class MyError extends Error {
    constructor(msg) {
        super();
        this.name = 'MyError';
        this.message = msg;
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 2);
    }


    #[test]
    fn allows_class_field_name() {
        let code = r#"
class MyError extends Error {
    name = 'MyError';
    constructor(message) {
        super(message);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }


    #[test]
    fn allows_non_error_class() {
        let code = r#"
class MyThing extends Base {
    constructor() {
        super();
        this.name = 'MyThing';
    }
}
"#;
        assert!(run_on(code).is_empty());
    }


    #[test]
    fn allows_empty_constructor() {
        let code = r#"
class MyError extends Error {
    name = 'MyError';
    constructor(msg) {
        super(msg);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }
}
