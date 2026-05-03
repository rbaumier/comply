//! prefer-type-error OXC backend — flag `throw new Error()` in type-checking conditions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Names of functions commonly used for type checking (on member expressions).
const TYPE_CHECK_IDENTIFIERS: &[&str] = &[
    "isArguments",
    "isArray",
    "isArrayBuffer",
    "isArrayLike",
    "isArrayLikeObject",
    "isBigInt",
    "isBoolean",
    "isBuffer",
    "isDate",
    "isElement",
    "isError",
    "isFinite",
    "isFunction",
    "isInteger",
    "isLength",
    "isMap",
    "isNaN",
    "isNative",
    "isNil",
    "isNull",
    "isNumber",
    "isObject",
    "isObjectLike",
    "isPlainObject",
    "isPrototypeOf",
    "isRegExp",
    "isSafeInteger",
    "isSet",
    "isString",
    "isSymbol",
    "isTypedArray",
    "isUndefined",
    "isView",
    "isWeakMap",
    "isWeakSet",
    "isWindow",
    "isXMLDoc",
];

const TYPE_CHECK_GLOBALS: &[&str] = &["isNaN", "isFinite"];

fn is_error_constructor_name(name: &str) -> bool {
    name.ends_with("Error") && name.starts_with(|c: char| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else {
            return;
        };

        // The thrown value must be `new Error(...)`.
        let Expression::NewExpression(new_expr) = &throw.argument else {
            return;
        };
        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        if ctor.name.as_str() != "Error" {
            return;
        }

        let nodes = semantic.nodes();

        // The throw must be the lone statement in its parent block.
        let parent_id = nodes.parent_id(node.id());
        if parent_id == node.id() {
            return;
        }
        let parent_kind = nodes.kind(parent_id);

        // Check if throw is the single statement in a block or directly under if
        let if_node_id = match parent_kind {
            AstKind::BlockStatement(block) => {
                if block.body.len() != 1 {
                    return;
                }
                // parent of block should be if_statement
                let grandparent_id = nodes.parent_id(parent_id);
                if grandparent_id == parent_id {
                    return;
                }
                match nodes.kind(grandparent_id) {
                    AstKind::IfStatement(_) => grandparent_id,
                    _ => return,
                }
            }
            // throw directly as consequence of if (no braces)
            AstKind::IfStatement(_) => parent_id,
            _ => return,
        };

        let AstKind::IfStatement(if_stmt) = nodes.kind(if_node_id) else {
            return;
        };

        // Check that the if condition is a type-checking expression.
        if !is_typechecking_expression(&if_stmt.test) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ctor.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new Error()` is too unspecific for a type check. \
                      Use `new TypeError()` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_typechecking_expression(expr: &Expression) -> bool {
    match expr {
        Expression::UnaryExpression(unary) => {
            if unary.operator == UnaryOperator::Typeof {
                return true;
            }
            if unary.operator == UnaryOperator::LogicalNot {
                return is_typechecking_expression(&unary.argument);
            }
            false
        }
        Expression::BinaryExpression(binary) => {
            if binary.operator == oxc_ast::ast::BinaryOperator::Instanceof {
                // Check right side — if it's an Error constructor, don't consider it a type check
                if let Some(name) = identifier_name(&binary.right) {
                    if is_error_constructor_name(name) {
                        return false;
                    }
                }
                if let Expression::StaticMemberExpression(member) = &binary.right {
                    if is_error_constructor_name(member.property.name.as_str()) {
                        return false;
                    }
                }
                return true;
            }
            is_typechecking_expression(&binary.left)
                || is_typechecking_expression(&binary.right)
        }
        Expression::CallExpression(call) => {
            if call.arguments.is_empty() {
                return false;
            }
            match &call.callee {
                Expression::Identifier(ident) => {
                    TYPE_CHECK_GLOBALS.contains(&ident.name.as_str())
                }
                Expression::StaticMemberExpression(member) => {
                    is_typecheck_member_expression(member)
                }
                _ => false,
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            is_typechecking_expression(&paren.expression)
        }
        _ => false,
    }
}

fn is_typecheck_member_expression(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    let name = member.property.name.as_str();
    if TYPE_CHECK_IDENTIFIERS.contains(&name) {
        return true;
    }
    if let Expression::StaticMemberExpression(inner) = &member.object {
        return is_typecheck_member_expression(inner);
    }
    false
}

fn identifier_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    if let Expression::Identifier(ident) = expr {
        Some(ident.name.as_str())
    } else {
        None
    }
}
