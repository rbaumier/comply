//! ts-no-enum-object-literal-pattern — OXC backend.
//! Flags `Color[someVar]` where `Color` is declared `as const`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Expression, IdentifierReference, TSType, TSTypeOperatorOperator,
    TSTypeQueryExprName, VariableDeclarationKind,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub struct Check;

/// Collect names of `const X = { ... } as const` bindings.
fn collect_as_const_objects<'a>(semantic: &'a oxc_semantic::Semantic<'a>) -> FxHashSet<&'a str> {
    let mut names = FxHashSet::default();
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

/// Collect `type Alias = keyof typeof Obj` declarations as `alias -> obj`.
fn collect_keyof_typeof_aliases<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> FxHashMap<&'a str, &'a str> {
    let mut aliases = FxHashMap::default();
    for node in semantic.nodes().iter() {
        let AstKind::TSTypeAliasDeclaration(decl) = node.kind() else { continue };
        if let Some(obj) = keyof_typeof_target(&decl.type_annotation) {
            aliases.insert(decl.id.name.as_str(), obj);
        }
    }
    aliases
}

/// If `ty` is `keyof typeof X`, return `X`'s name; otherwise `None`.
fn keyof_typeof_target<'a>(ty: &'a TSType<'a>) -> Option<&'a str> {
    let TSType::TSTypeOperatorType(op) = ty else { return None };
    if op.operator != TSTypeOperatorOperator::Keyof {
        return None;
    }
    let TSType::TSTypeQuery(query) = &op.type_annotation else { return None };
    match &query.expr_name {
        TSTypeQueryExprName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True when `ty` is `keyof typeof obj_name`, either directly or through a
/// type alias that resolves to it.
fn type_keys_obj(ty: &TSType, obj_name: &str, aliases: &FxHashMap<&str, &str>) -> bool {
    if keyof_typeof_target(ty) == Some(obj_name) {
        return true;
    }
    if let TSType::TSTypeReference(r) = ty
        && let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &r.type_name
    {
        return aliases.get(id.name.as_str()) == Some(&obj_name);
    }
    false
}

/// True when the index identifier's declared type is `keyof typeof obj_name`
/// (directly or via alias) — the lookup is then statically key-narrow and safe.
fn index_ident_keys_obj<'a>(
    id: &IdentifierReference<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        let ann = match kind {
            AstKind::FormalParameter(param) => param.type_annotation.as_ref(),
            AstKind::VariableDeclarator(decl) => decl.type_annotation.as_ref(),
            _ => continue,
        };
        return ann.is_some_and(|a| type_keys_obj(&a.type_annotation, obj_name, aliases));
    }
    false
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

        // A variable typed `keyof typeof Obj` (directly or via a type alias)
        // makes the lookup statically key-narrow — the canonical, correct way
        // to read an `as const` map. Not the widening enum-replacement pattern.
        if let Expression::Identifier(idx_id) = &member.expression {
            let aliases = collect_keyof_typeof_aliases(semantic);
            if index_ident_keys_obj(idx_id, obj_name, semantic, &aliases) {
                return;
            }
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

    #[test]
    fn allows_key_typed_via_keyof_typeof_alias() {
        // Regression for issue #556: `value: Breakpoint` where
        // `type Breakpoint = keyof typeof BREAKPOINTS` is the canonical,
        // key-narrow lookup — not the widening enum pattern.
        let src = "const BREAKPOINTS = { sm: 640, md: 800 } as const;\n\
                   type Breakpoint = keyof typeof BREAKPOINTS;\n\
                   function resolve(value: Breakpoint): number { return BREAKPOINTS[value]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_key_typed_directly_as_keyof_typeof() {
        let src = "const BREAKPOINTS = { sm: 640, md: 800 } as const;\n\
                   function resolve(value: keyof typeof BREAKPOINTS): number { return BREAKPOINTS[value]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_key_const_typed_as_keyof_typeof() {
        let src = "const BREAKPOINTS = { sm: 640, md: 800 } as const;\n\
                   const key: keyof typeof BREAKPOINTS = 'sm';\n\
                   const v = BREAKPOINTS[key];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_alias_keying_a_different_object() {
        // `keyof typeof OTHER` does not make `BREAKPOINTS[value]` safe.
        let src = "const BREAKPOINTS = { sm: 640 } as const;\n\
                   const OTHER = { a: 1 } as const;\n\
                   type K = keyof typeof OTHER;\n\
                   function f(value: K) { return BREAKPOINTS[value]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_string_typed_key() {
        let src = "const BREAKPOINTS = { sm: 640 } as const;\n\
                   function f(value: string) { return BREAKPOINTS[value]; }";
        assert_eq!(run(src).len(), 1);
    }
}
