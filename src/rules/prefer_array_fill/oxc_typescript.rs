//! prefer-array-fill oxc backend — flag `Array.from({length: n}, () => constant)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Array.from"])
    }

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is `Array.from`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Array" || member.property.name.as_str() != "from" {
            return;
        }

        // Exactly 2 arguments
        if call.arguments.len() != 2 {
            return;
        }

        // First arg: { length: n }
        let Argument::ObjectExpression(first) = &call.arguments[0] else {
            return;
        };
        if first.properties.len() != 1 {
            return;
        }
        let ObjectPropertyKind::ObjectProperty(prop) = &first.properties[0] else {
            return;
        };
        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => return,
        };
        if key_name != "length" {
            return;
        }

        // Second arg: arrow function returning a constant literal
        let Argument::ArrowFunctionExpression(arrow) = &call.arguments[1] else {
            return;
        };

        // Arrow must have no parameters (or only unused ones)
        // Check body is a simple expression (not a block)
        if arrow.expression {
            // expression body — check if it's a constant literal
            let stmts = &arrow.body.statements;
            if stmts.len() != 1 {
                return;
            }
            let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = &stmts[0] else {
                return;
            };
            let is_constant = matches!(
                &expr_stmt.expression,
                Expression::NumericLiteral(_)
                    | Expression::StringLiteral(_)
                    | Expression::BooleanLiteral(_)
                    | Expression::NullLiteral(_)
            );
            if !is_constant {
                return;
            }
        } else {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `Array(n).fill(value)` instead of `Array.from({length: n}, () => value)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(code, &Check)
    }


    #[test]
    fn flags_array_from_constant() {
        assert_eq!(run("Array.from({length: 5}, () => 0)").len(), 1);
        assert_eq!(run("Array.from({length: n}, () => null)").len(), 1);
    }


    #[test]
    fn allows_array_from_with_index() {
        // Uses index parameter, can't use fill
        assert!(run("Array.from({length: 5}, (_, i) => i)").is_empty());
    }


    #[test]
    fn allows_array_fill() {
        assert!(run("Array(5).fill(0)").is_empty());
    }


    #[test]
    fn allows_array_from_iterable() {
        assert!(run("Array.from(set)").is_empty());
    }
}
