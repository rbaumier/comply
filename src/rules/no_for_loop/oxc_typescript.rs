//! no-for-loop OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
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

/// Get the index variable name and its resolved symbol from the `for` init
/// clause. Expects `let i = 0` or `var i = 0`. The symbol is `None` when the
/// binding is unresolved, in which case the body-usage check is skipped.
fn get_index_name<'a>(
    init: &'a ForStatementInit<'a>,
) -> Option<(&'a str, Option<oxc_semantic::SymbolId>)> {
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
    Some((id.name.as_str(), id.symbol_id.get()))
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

/// True when the loop index `symbol_id` is referenced anywhere in `body` in a
/// position other than the computed key of a subscript (`arr[i]`). A call
/// argument (`args.slice(i)`), a `return i`, or an arithmetic expression needs
/// the numeric position, which a `for-of` loop cannot provide, so the rewrite
/// suggestion would be wrong.
///
/// References in the loop header (`i < arr.length`, `i++`) lie outside the body
/// span and are not considered.
fn index_used_beyond_subscript(
    body: &Statement,
    symbol_id: oxc_semantic::SymbolId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let body_span = body.span();
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol_id).any(|reference| {
        let ref_span = nodes.get_node(reference.node_id()).kind().span();
        // Only references inside the loop body count; skip the header clauses.
        if ref_span.start < body_span.start || ref_span.end > body_span.end {
            return false;
        }
        !is_subscript_key_use(nodes.kind(nodes.parent_id(reference.node_id())), ref_span)
    })
}

/// True when a reference (spanning `ref_span`) is exactly the computed key of a
/// member access — the `i` in `arr[i]`. The span equality pins the reference to
/// the index position, so `arr[i + 1]` (the key is the binary expression, not
/// the bare `i`) does not qualify.
fn is_subscript_key_use(parent: AstKind<'_>, ref_span: oxc_span::Span) -> bool {
    matches!(
        parent,
        AstKind::ComputedMemberExpression(member) if member.expression.span() == ref_span
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForStatement(for_stmt) = node.kind() else {
            return;
        };

        // 1. Extract index variable from initializer
        let Some(ref init) = for_stmt.init else {
            return;
        };
        let Some((idx_name, idx_symbol)) = get_index_name(init) else {
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

        // 4. The index must be used only as `arr[i]` subscript inside the body.
        // Any other use (slice(i), return i, arithmetic) needs the numeric
        // position, which `for-of` cannot supply, so the rewrite is wrong.
        if let Some(symbol_id) = idx_symbol
            && index_used_beyond_subscript(&for_stmt.body, symbol_id, semantic)
        {
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
            severity: Severity::Error,
            span: None,
        });
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
    fn flags_pure_subscript_loop() {
        // `i` used only as `arr[i]` — genuinely a `for-of` candidate.
        let d = run_on("for (let i = 0; i < arr.length; i++) { console.log(arr[i]); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-for-loop");
    }

    #[test]
    fn allows_index_passed_to_slice() {
        // #6488: `args.slice(i)` needs the position, not just the element.
        let src = r#"for (let i = 0; i < args.length; i++) {
            const arg = args[i]!;
            if (arg === "--") {
                processedArgs.push(...args.slice(i));
                break;
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_index_returned() {
        // #6488: `return i` yields the numeric index, not the element.
        let src = r#"function findSubCommandIndex(rawArgs, argsDef) {
            for (let i = 0; i < rawArgs.length; i++) {
                const arg = rawArgs[i]!;
                if (arg.startsWith("-")) { continue; }
                return i;
            }
            return -1;
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_pure_subscript_single_statement_body() {
        // Non-block body: header refs still precede the body, `arr[i]` is the
        // only body ref, so the loop is still a `for-of` candidate.
        let d = run_on("for (let i = 0; i < arr.length; i++) console.log(arr[i]);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_index_in_arithmetic() {
        // `arr[i + 1]` — the key is `i + 1`, not the bare index.
        let src = "for (let i = 0; i < arr.length; i++) { console.log(arr[i + 1]); }";
        assert!(run_on(src).is_empty());
    }
}
