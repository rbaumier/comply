//! OxcCheck backend for react-no-sort-for-extrema.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_sort_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "sort"
}

fn is_zero(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(n) => n.value == 0.0 && n.raw.as_ref().is_some_and(|r| r == "0"),
        _ => false,
    }
}

fn is_length_minus_one(expr: &Expression) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    if bin.operator != oxc_ast::ast::BinaryOperator::Subtraction {
        return false;
    }
    // right must be `1`
    let Expression::NumericLiteral(right) = &bin.right else {
        return false;
    };
    if right.value != 1.0 {
        return false;
    }
    // left must be `<something>.length`
    let Expression::StaticMemberExpression(left) = &bin.left else {
        return false;
    };
    left.property.name.as_str() == "length"
}

/// Walk ancestors to find if this identifier was bound to a `.sort()` call
/// in a preceding variable declaration.
fn identifier_bound_to_sort<'a>(
    node: &oxc_semantic::AstNode<'a>,
    name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up to find an enclosing block/program
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::FunctionBody(body) => {
                for stmt in &body.statements {
                    if let oxc_ast::ast::Statement::VariableDeclaration(decl) = stmt {
                        for declarator in &decl.declarations {
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                                &declarator.id
                            {
                                if id.name.as_str() == name {
                                    if let Some(init) = &declarator.init {
                                        return is_sort_call(init);
                                    }
                                }
                            }
                        }
                    }
                }
                return false;
            }
            AstKind::Program(program) => {
                for stmt in &program.body {
                    if let oxc_ast::ast::Statement::VariableDeclaration(decl) = stmt {
                        for declarator in &decl.declarations {
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                                &declarator.id
                            {
                                if id.name.as_str() == name {
                                    if let Some(init) = &declarator.init {
                                        return is_sort_call(init);
                                    }
                                }
                            }
                        }
                    }
                }
                return false;
            }
            _ => continue,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(subscript) = node.kind() else {
            return;
        };
        let expr = &subscript.expression;
        if !is_zero(expr) && !is_length_minus_one(expr) {
            return;
        }

        let direct_sort = is_sort_call(&subscript.object);
        let aliased_sort = if let Expression::Identifier(ident) = &subscript.object {
            identifier_bound_to_sort(node, ident.name.as_str(), semantic)
        } else {
            false
        };

        if !direct_sort && !aliased_sort {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, subscript.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.sort(...)[0]` / `.sort(...)[length-1]` picks an extremum via O(n log n) work — \
                      use `Math.min` / `Math.max` or a single-pass fold."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
