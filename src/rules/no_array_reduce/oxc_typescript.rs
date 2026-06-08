//! no-array-reduce OxcCheck backend — flag complex `.reduce()` / `.reduceRight()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const METHODS: &[&str] = &["reduce", "reduceRight"];
const SIMPLE_OPS: &[oxc_ast::ast::BinaryOperator; 6] = &[
    oxc_ast::ast::BinaryOperator::Addition,
    oxc_ast::ast::BinaryOperator::Subtraction,
    oxc_ast::ast::BinaryOperator::Multiplication,
    oxc_ast::ast::BinaryOperator::Division,
    oxc_ast::ast::BinaryOperator::Remainder,
    oxc_ast::ast::BinaryOperator::Exponential,
];

fn is_simple_arithmetic(call: &oxc_ast::ast::CallExpression, source: &str) -> bool {
    // Find arrow or function expression callback.
    let cb = call.arguments.iter().find_map(|arg| match arg {
        oxc_ast::ast::Argument::ArrowFunctionExpression(a) => Some(&**a as &dyn HasBody),
        oxc_ast::ast::Argument::FunctionExpression(f) => Some(&**f as &dyn HasBody),
        _ => None,
    });
    let Some(cb) = cb else { return false };
    cb.is_simple_arithmetic(source)
}

trait HasBody {
    fn is_simple_arithmetic(&self, source: &str) -> bool;
}

impl HasBody for oxc_ast::ast::ArrowFunctionExpression<'_> {
    fn is_simple_arithmetic(&self, source: &str) -> bool {
        if self.expression {
            // Concise body: single expression statement.
            if self.body.statements.len() != 1 {
                return false;
            }
            let oxc_ast::ast::Statement::ExpressionStatement(stmt) = &self.body.statements[0]
            else {
                return false;
            };
            return is_simple_binary_or_math(&stmt.expression, source);
        }
        // Block body with single return.
        let stmts: Vec<_> = self
            .body
            .statements
            .iter()
            .filter(|s| !matches!(s, oxc_ast::ast::Statement::EmptyStatement(_)))
            .collect();
        if stmts.len() != 1 {
            return false;
        }
        let oxc_ast::ast::Statement::ReturnStatement(ret) = stmts[0] else {
            return false;
        };
        ret.argument
            .as_ref()
            .is_some_and(|e| is_simple_binary_or_math(e, source))
    }
}

impl HasBody for oxc_ast::ast::Function<'_> {
    fn is_simple_arithmetic(&self, source: &str) -> bool {
        let Some(body) = &self.body else { return false };
        let stmts: Vec<_> = body
            .statements
            .iter()
            .filter(|s| !matches!(s, oxc_ast::ast::Statement::EmptyStatement(_)))
            .collect();
        if stmts.len() != 1 {
            return false;
        }
        let oxc_ast::ast::Statement::ReturnStatement(ret) = stmts[0] else {
            return false;
        };
        ret.argument
            .as_ref()
            .is_some_and(|e| is_simple_binary_or_math(e, source))
    }
}

fn is_simple_binary_or_math(expr: &Expression, source: &str) -> bool {
    if let Expression::BinaryExpression(bin) = expr {
        return SIMPLE_OPS.contains(&bin.operator);
    }
    // Math.min / Math.max
    if let Expression::CallExpression(call) = expr
        && let Expression::StaticMemberExpression(member) = &call.callee {
            use oxc_span::GetSpan;
            let obj_text = &source
                [member.object.span().start as usize..member.object.span().end as usize];
            let prop = member.property.name.as_str();
            if obj_text == "Math" && (prop == "min" || prop == "max") {
                return true;
            }
        }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["reduce"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !METHODS.contains(&method) {
            return;
        }

        if is_simple_arithmetic(call, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`Array#{}()` with complex logic is hard to read — use a `for...of` loop instead. \
                 Simple arithmetic reduces like `(sum, n) => sum + n` are allowed.",
                method
            ),
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
    fn allows_simple_sum() {
        assert!(run_on("const sum = arr.reduce((acc, x) => acc + x, 0);").is_empty());
    }


    #[test]
    fn allows_simple_product() {
        assert!(run_on("const prod = arr.reduce((acc, x) => acc * x, 1);").is_empty());
    }


    #[test]
    fn allows_simple_sum_block_body() {
        assert!(run_on("const sum = arr.reduce((acc, x) => { return acc + x; }, 0);").is_empty());
    }


    #[test]
    fn allows_math_min() {
        assert!(run_on("const min = arr.reduce((a, b) => Math.min(a, b));").is_empty());
    }


    #[test]
    fn flags_complex_reduce() {
        assert_eq!(
            run_on("const obj = arr.reduce((acc, x) => ({ ...acc, [x.id]: x }), {});").len(),
            1
        );
    }


    #[test]
    fn flags_reduce_right_complex() {
        assert_eq!(
            run_on("const r = arr.reduceRight((acc, x) => acc.concat(x.items), []);").len(),
            1
        );
    }


    #[test]
    fn allows_non_reduce() {
        assert!(run_on("const x = arr.map(x => x * 2);").is_empty());
    }


    #[test]
    fn allows_direct_function_call() {
        assert!(run_on("reduce(acc, x);").is_empty());
    }
}
