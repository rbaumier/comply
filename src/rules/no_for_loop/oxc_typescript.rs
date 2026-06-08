//! no-for-loop OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

/// Check if an expression is the numeric literal `0`.
fn is_literal_zero(expr: &Expression) -> bool {
    if let Expression::NumericLiteral(lit) = expr {
        lit.value == 0.0
    } else {
        false
    }
}

/// Check if an expression is the numeric literal `1`.
fn is_literal_one(expr: &Expression) -> bool {
    if let Expression::NumericLiteral(lit) = expr {
        lit.value == 1.0
    } else {
        false
    }
}

/// Get the index variable name from the `for` init clause.
/// Expects `let i = 0` or `var i = 0`.
fn get_index_name<'a>(init: &'a ForStatementInit<'a>) -> Option<&'a str> {
    let ForStatementInit::VariableDeclaration(decl) = init else {
        return None;
    };
    if decl.declarations.len() != 1 {
        return None;
    }
    let d = &decl.declarations[0];
    let BindingPattern::BindingIdentifier(ref id) = d.id else {
        return None;
    };
    let init_expr = d.init.as_ref()?;
    if !is_literal_zero(init_expr) {
        return None;
    }
    Some(id.name.as_str())
}

/// Get the text of an identifier expression.
fn ident_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    if let Expression::Identifier(id) = expr {
        Some(id.name.as_str())
    } else {
        None
    }
}

/// Check condition is `i < arr.length` or `arr.length > i`.
fn check_condition<'a>(test: &'a Expression<'a>, idx_name: &str) -> bool {
    let Expression::BinaryExpression(bin) = test else {
        return false;
    };

    let (lesser, greater) = match bin.operator {
        BinaryOperator::LessThan => (&bin.left, &bin.right),
        BinaryOperator::GreaterThan => (&bin.right, &bin.left),
        _ => return false,
    };

    // lesser must be the index identifier
    if ident_name(lesser) != Some(idx_name) {
        return false;
    }

    // greater must be `arr.length`
    if let Expression::StaticMemberExpression(member) = greater {
        member.property.name.as_str() == "length"
    } else {
        false
    }
}

/// Check the update is `i++`, `++i`, `i += 1`, or `i = i + 1`.
fn check_update<'a>(update: &'a Expression<'a>, idx_name: &str) -> bool {
    match update {
        Expression::UpdateExpression(up) => {
            // i++ or ++i
            if !matches!(up.operator, UpdateOperator::Increment) {
                return false;
            }
            match &up.argument {
                SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                    id.name.as_str() == idx_name
                }
                _ => false,
            }
        }
        Expression::AssignmentExpression(assign) => {
            // LHS must be the index variable
            let lhs_name = match &assign.left {
                AssignmentTarget::AssignmentTargetIdentifier(id) => Some(id.name.as_str()),
                _ => None,
            };
            if lhs_name != Some(idx_name) {
                return false;
            }

            match assign.operator {
                AssignmentOperator::Addition => {
                    // i += 1
                    is_literal_one(&assign.right)
                }
                AssignmentOperator::Assign => {
                    // i = i + 1 or i = 1 + i
                    if let Expression::BinaryExpression(bin) = &assign.right {
                        if bin.operator != BinaryOperator::Addition {
                            return false;
                        }
                        (ident_name(&bin.left) == Some(idx_name) && is_literal_one(&bin.right))
                            || (is_literal_one(&bin.left)
                                && ident_name(&bin.right) == Some(idx_name))
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForStatement(for_stmt) = node.kind() else {
            return;
        };

        // 1. Extract index variable from initializer
        let Some(ref init) = for_stmt.init else {
            return;
        };
        let Some(idx_name) = get_index_name(init) else {
            return;
        };

        // 2. Check condition: `i < arr.length`
        let Some(ref test) = for_stmt.test else {
            return;
        };
        if !check_condition(test, idx_name) {
            return;
        }

        // 3. Check update: `i++`, `++i`, `i += 1`, `i = i + 1`
        let Some(ref update) = for_stmt.update else {
            return;
        };
        if !check_update(update, idx_name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, for_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use a `for-of` loop instead of this `for` loop.".into(),
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
    fn flags_classic_for_loop() {
        let d = run_on("for (let i = 0; i < arr.length; i++) { console.log(arr[i]); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-for-loop");
    }


    #[test]
    fn flags_var_for_loop() {
        let d = run_on("for (var i = 0; i < items.length; i++) { use(items[i]); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_plus_equals_increment() {
        let d = run_on("for (let i = 0; i < arr.length; i += 1) { f(arr[i]); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_prefix_increment() {
        let d = run_on("for (let i = 0; i < arr.length; ++i) { f(arr[i]); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_reversed_condition() {
        let d = run_on("for (let i = 0; arr.length > i; i++) { f(arr[i]); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_for_of() {
        assert!(run_on("for (const item of arr) { console.log(item); }").is_empty());
    }


    #[test]
    fn allows_for_in() {
        assert!(run_on("for (const key in obj) { console.log(key); }").is_empty());
    }


    #[test]
    fn allows_non_zero_init() {
        assert!(run_on("for (let i = 1; i < arr.length; i++) { f(arr[i]); }").is_empty());
    }


    #[test]
    fn allows_decrement() {
        assert!(run_on("for (let i = 0; i < arr.length; i--) { f(arr[i]); }").is_empty());
    }


    #[test]
    fn allows_step_two() {
        assert!(run_on("for (let i = 0; i < arr.length; i += 2) { f(arr[i]); }").is_empty());
    }


    #[test]
    fn allows_non_length_condition() {
        assert!(run_on("for (let i = 0; i < 10; i++) { f(i); }").is_empty());
    }
}
