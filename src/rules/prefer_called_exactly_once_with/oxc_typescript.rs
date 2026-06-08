//! prefer-called-exactly-once-with OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Info extracted from an `expect(x).MATCHER(args)` call expression.
struct ExpectCall {
    /// Source text of the argument passed to `expect(...)`.
    expect_arg: String,
    /// Matcher name (e.g. `toHaveBeenCalledTimes`).
    matcher: String,
    /// Source text of matcher arguments (to check for literal `1`).
    matcher_args_text: String,
    /// Byte offset of the entire expression statement.
    span_start: u32,
}

/// Try to parse a CallExpression as `expect(x).MATCHER(args)`.
fn parse_expect_call(call: &oxc_ast::ast::CallExpression, source: &str) -> Option<ExpectCall> {
    // Callee must be a static member: `expect(x).MATCHER`
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let matcher = member.property.name.as_str().to_string();

    // Object must be a call to `expect`
    let oxc_ast::ast::Expression::CallExpression(expect_call) = &member.object else {
        return None;
    };
    let oxc_ast::ast::Expression::Identifier(expect_ident) = &expect_call.callee else {
        return None;
    };
    if expect_ident.name.as_str() != "expect" {
        return None;
    }
    if expect_call.arguments.len() != 1 {
        return None;
    }
    let arg = &expect_call.arguments[0];
    let arg_span = arg.span();
    let expect_arg = source[arg_span.start as usize..arg_span.end as usize].to_string();

    // Matcher arguments text
    let mut matcher_args_text = String::new();
    for a in &call.arguments {
        let s = a.span();
        matcher_args_text.push_str(&source[s.start as usize..s.end as usize]);
        matcher_args_text.push(',');
    }

    Some(ExpectCall {
        expect_arg,
        matcher,
        matcher_args_text,
        span_start: call.span.start,
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toHaveBeenCalledTimes"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Collect all ExpressionStatements that are `expect(x).MATCHER(args)` calls.
        // We need to check consecutive siblings in statement blocks.
        // Walk all nodes and find statement blocks / program.
        for node in semantic.nodes().iter() {
            let stmts: &[oxc_ast::ast::Statement] = match node.kind() {
                AstKind::Program(prog) => &prog.body,
                AstKind::FunctionBody(body) => &body.statements,
                _ => continue,
            };

            if stmts.len() < 2 {
                continue;
            }

            for i in 0..stmts.len() - 1 {
                let first = &stmts[i];
                let second = &stmts[i + 1];

                // Both must be expression statements containing call expressions
                let oxc_ast::ast::Statement::ExpressionStatement(first_expr) = first else {
                    continue;
                };
                let oxc_ast::ast::Statement::ExpressionStatement(second_expr) = second else {
                    continue;
                };
                let oxc_ast::ast::Expression::CallExpression(first_call) = &first_expr.expression
                else {
                    continue;
                };
                let oxc_ast::ast::Expression::CallExpression(second_call) = &second_expr.expression
                else {
                    continue;
                };

                let Some(a) = parse_expect_call(first_call, ctx.source) else {
                    continue;
                };
                if a.matcher != "toHaveBeenCalledTimes" {
                    continue;
                }
                // Check that args is a single `1`
                if a.matcher_args_text.trim_end_matches(',').trim() != "1" {
                    continue;
                }

                let Some(b) = parse_expect_call(second_call, ctx.source) else {
                    continue;
                };
                if b.matcher != "toHaveBeenCalledWith" {
                    continue;
                }
                if a.expect_arg != b.expect_arg {
                    continue;
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, a.span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Replace `toHaveBeenCalledTimes(1)` + `toHaveBeenCalledWith(...)` on `{}` with `toHaveBeenCalledExactlyOnceWith(...)`.",
                        a.expect_arg
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
    use crate::rules::test_helpers::run_oxc_ts;



    #[test]
    fn flags_times_one_then_called_with_same_mock() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledTimes(1);
                expect(fn).toHaveBeenCalledWith(1, 2);
            });
        "#;
        let d = run_oxc_ts(src, &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toHaveBeenCalledExactlyOnceWith"));
    }


    #[test]
    fn ignores_non_consecutive_statements() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledTimes(1);
                doSomething();
                expect(fn).toHaveBeenCalledWith(1, 2);
            });
        "#;
        let d = run_oxc_ts(src, &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn ignores_different_mocks() {
        let src = r#"
            test('x', () => {
                expect(a).toHaveBeenCalledTimes(1);
                expect(b).toHaveBeenCalledWith(1);
            });
        "#;
        let d = run_oxc_ts(src, &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn ignores_times_not_equal_to_one() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledTimes(2);
                expect(fn).toHaveBeenCalledWith(1);
            });
        "#;
        let d = run_oxc_ts(src, &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn ignores_reversed_order() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledWith(1);
                expect(fn).toHaveBeenCalledTimes(1);
            });
        "#;
        let d = run_oxc_ts(src, &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn flags_at_program_top_level() {
        let src = "expect(fn).toHaveBeenCalledTimes(1);\nexpect(fn).toHaveBeenCalledWith(42);\n";
        let d = run_oxc_ts(src, &Check);
        assert_eq!(d.len(), 1);
    }
}
