//! no-associative-arrays oxc backend — flag string-keyed assignment on arrays.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, Expression, VariableDeclarationKind,
};
use std::sync::Arc;

pub struct Check;

/// Check if a value expression is an array literal or `new Array(...)`.
fn is_array_init(expr: &Expression) -> bool {
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new) => {
            if let Expression::Identifier(id) = &new.callee {
                id.name.as_str() == "Array"
            } else {
                false
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };

        // Left side must be a computed member (subscript) with a string literal index.
        let AssignmentTarget::ComputedMemberExpression(computed) = &assign.left else {
            return;
        };
        // Index must be a string literal.
        let Expression::StringLiteral(_) = &computed.expression else {
            return;
        };
        // Object must be an identifier.
        let Expression::Identifier(obj_id) = &computed.object else {
            return;
        };
        let var_name = obj_id.name.as_str();

        // Walk semantic nodes to find a variable declaration that initialises
        // this name as an array, in an enclosing scope.
        let mut found_array_decl = false;
        for snode in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = snode.kind() else {
                continue;
            };
            if !matches!(
                decl.kind,
                VariableDeclarationKind::Const
                    | VariableDeclarationKind::Let
                    | VariableDeclarationKind::Var
            ) {
                continue;
            }
            for declarator in &decl.declarations {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = declarator.id
                {
                    if id.name.as_str() != var_name {
                        continue;
                    }
                    if let Some(ref init) = declarator.init
                        && is_array_init(init) {
                            found_array_decl = true;
                            break;
                        }
                }
            }
            if found_array_decl {
                break;
            }
        }

        if !found_array_decl {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Array `{var_name}` is used as an associative array — use a Map or plain object instead."
            ),
            severity: Severity::Error,
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
    fn flags_bracket_string_key_assignment() {
        let src = "const arr = [];\narr[\"key\"] = 1;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_single_quote_bracket_key() {
        let src = "let items = [];\nitems['name'] = \"hello\";";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_numeric_index() {
        let src = "const arr = [];\narr[0] = 1;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_non_array_bracket_access() {
        let src = "const obj = {};\nobj[\"key\"] = 1;";
        assert!(run_on(src).is_empty());
    }
}
