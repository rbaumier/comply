//! ts-no-enum-object-literal-pattern — OXC backend.
//! Flags `Color[someVar]` where `Color` is declared `as const`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, TSType, VariableDeclarationKind};
use oxc_span::GetSpan;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

/// Collect names of `const X = { ... } as const` bindings.
fn collect_as_const_objects<'a>(semantic: &'a oxc_semantic::Semantic<'a>) -> HashSet<&'a str> {
    let mut names = HashSet::new();
    for node in semantic.nodes().iter() {
        let AstKind::VariableDeclaration(decl) = node.kind() else { continue };
        if decl.kind != VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &decl.declarations {
            let Some(init) = &declarator.init else { continue };
            // Must be `expr as const` — a TSAsExpression.
            let Expression::TSAsExpression(as_expr) = init else { continue };
            // The type annotation must be TSTypeReference for `const` keyword.
            let is_as_const = matches!(&as_expr.type_annotation, TSType::TSTypeReference(r) if {
                let name = &r.type_name;
                matches!(name, oxc_ast::ast::TSTypeName::IdentifierReference(id) if id.name.as_str() == "const")
            });
            if !is_as_const {
                continue;
            }
            // The expression part should be an object.
            let Expression::ObjectExpression(_) = &as_expr.expression else { continue };
            // Get the binding name.
            if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                names.insert(id.name.as_str());
            }
        }
    }
    names
}

/// Is the index expression a safe literal (string, number) or a `keyof` cast?
fn is_safe_index(expr: &Expression, source: &str) -> bool {
    match expr {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TSAsExpression(as_expr) => {
            let span = as_expr.span;
            let text = &source[span.start as usize..span.end as usize];
            text.contains("keyof ")
        }
        Expression::TSTypeAssertion(ta) => {
            let span = ta.span;
            let text = &source[span.start as usize..span.end as usize];
            text.contains("keyof ")
        }
        _ => false,
    }
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
        let AstKind::ComputedMemberExpression(member) = node.kind() else { return };

        let Expression::Identifier(obj_id) = &member.object else { return };
        let obj_name = obj_id.name.as_str();

        if is_safe_index(&member.expression, ctx.source) {
            return;
        }

        let names = collect_as_const_objects(semantic);
        if !names.contains(obj_name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Indexing `{obj_name}` (declared `as const`) with an arbitrary key widens the result \
                 to a unioned type and skips the narrow lookup. Cast: `{obj_name}[k as keyof typeof {obj_name}]`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_arbitrary_string_index() {
        let src = "const Color = { red: 'r', blue: 'b' } as const;\nfunction f(k: string) { return Color[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_string_literal_index() {
        let src = "const Color = { red: 'r', blue: 'b' } as const;\nconst v = Color['red'];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyof_cast_index() {
        let src = "const Color = { red: 'r' } as const;\nfunction f(k: string) { return Color[k as keyof typeof Color]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_as_const_object() {
        let src =
            "const Color = { red: 'r', blue: 'b' };\nfunction f(k: string) { return Color[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_indexing() {
        let src = "function f(arr: string[], i: number) { return arr[i]; }";
        assert!(run(src).is_empty());
    }
}
