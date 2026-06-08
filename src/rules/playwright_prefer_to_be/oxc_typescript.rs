//! playwright-prefer-to-be OXC backend — suggest `toBe()` for primitive literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];
const EQUALITY_MATCHERS: &[&str] = &["toEqual", "toStrictEqual"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Walk up a call/member chain looking for `expect(`.
fn is_expect_chain(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::Identifier(id) = &call.callee {
                return id.name.as_str() == "expect";
            }
            is_expect_chain(&call.callee)
        }
        Expression::StaticMemberExpression(member) => is_expect_chain(&member.object),
        _ => false,
    }
}

fn is_primitive_literal(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::TemplateLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            // -1, +2
            matches!(&unary.argument, Expression::NumericLiteral(_))
        }
        Expression::Identifier(id) => {
            matches!(id.name.as_str(), "undefined" | "NaN")
        }
        _ => false,
    }
}

fn suggested_matcher(expr: &Expression) -> &'static str {
    match expr {
        Expression::NullLiteral(_) => "toBeNull",
        Expression::Identifier(id) => match id.name.as_str() {
            "undefined" => "toBeUndefined",
            "NaN" => "toBeNaN",
            _ => "toBe",
        },
        _ => "toBe",
    }
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `<expect-chain>.<matcher>(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let matcher = member.property.name.as_str();
        if !EQUALITY_MATCHERS.contains(&matcher) {
            return;
        }
        if !is_expect_chain(&member.object) {
            return;
        }

        // First argument must be a primitive literal.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let arg_expr = match first_arg {
            Argument::SpreadElement(_) => return,
            _ => first_arg.to_expression(),
        };
        if !is_primitive_literal(arg_expr) {
            return;
        }

        let suggested = suggested_matcher(arg_expr);
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Use `{suggested}` when expecting primitive literals."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";


    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }


    #[test]
    fn flags_to_equal_with_number() {
        let d = run_oxc_ts("expect(x).toEqual(1);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBe"));
    }


    #[test]
    fn flags_to_equal_null() {
        let d = run_oxc_ts("expect(x).toEqual(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeNull"));
    }


    #[test]
    fn allows_to_equal_with_object() {
        let d = run_oxc_ts("expect(x).toEqual({a: 1});");
        assert!(d.is_empty());
    }


    #[test]
    fn allows_to_be() {
        let d = run_oxc_ts("expect(x).toBe(1);");
        assert!(d.is_empty());
    }
}
