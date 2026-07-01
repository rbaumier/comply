//! ts-no-enum-object-literal-pattern — OXC backend.
//! Flags `Color[someVar]` where `Color` is declared `as const`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Expression, IdentifierReference, TSTupleElement, TSType, TSTypeOperatorOperator,
    TSTypeQueryExprName, VariableDeclarationKind,
};
use oxc_span::GetSpan;
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

/// If `ty` is a bare type reference to an identifier, return its name.
fn type_ref_name<'a>(ty: &'a TSType<'a>) -> Option<&'a str> {
    let TSType::TSTypeReference(r) = ty else { return None };
    let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &r.type_name else { return None };
    Some(id.name.as_str())
}

/// True when the generic type parameter named `param_name`, declared on the
/// nearest function ancestor of `decl_node_id` that declares it, has a
/// constraint that resolves to `keyof typeof obj_name` (directly or via alias).
/// In valid TypeScript that nearest declarer is the function owning the indexed
/// parameter, so an unrelated same-named `T` cannot apply.
fn type_param_constraint_keys_obj<'a>(
    param_name: &str,
    decl_node_id: oxc_semantic::NodeId,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(decl_node_id) {
        let type_params = match kind {
            AstKind::Function(f) => f.type_parameters.as_deref(),
            AstKind::ArrowFunctionExpression(f) => f.type_parameters.as_deref(),
            _ => continue,
        };
        let Some(type_params) = type_params else { continue };
        let Some(tp) = type_params.params.iter().find(|tp| tp.name.name.as_str() == param_name)
        else {
            continue;
        };
        return tp
            .constraint
            .as_ref()
            .is_some_and(|c| type_keys_obj(c, obj_name, aliases));
    }
    false
}

/// Strip `TSParenthesizedType` wrappers. The parser preserves parentheses, so
/// the element type of `(keyof typeof X)[]` is a parenthesized node around the
/// `keyof typeof` operator.
fn skip_parens<'r, 'a>(mut ty: &'r TSType<'a>) -> &'r TSType<'a> {
    while let TSType::TSParenthesizedType(p) = ty {
        ty = &p.type_annotation;
    }
    ty
}

/// True when `expr` is a `TSAsExpression` casting to `(keyof typeof obj_name)[]`
/// — a `TSArrayType` whose element type resolves to `keyof typeof obj_name`.
fn as_expr_is_keyof_array(
    expr: &Expression,
    obj_name: &str,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    let Expression::TSAsExpression(as_expr) = expr else { return false };
    let TSType::TSArrayType(arr) = &as_expr.type_annotation else { return false };
    type_keys_obj(skip_parens(&arr.element_type), obj_name, aliases)
}

/// True when `expr` is — or is an identifier resolving (a single hop) to a
/// `VariableDeclarator` whose initializer is — a cast to `(keyof typeof
/// obj_name)[]`. Such an array's elements are statically known keys, so a value
/// taken out of it by element access is itself a known key.
fn array_elem_keys_obj<'a>(
    expr: &Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    if as_expr_is_keyof_array(expr, obj_name, aliases) {
        return true;
    }
    let Expression::Identifier(id) = expr else { return false };
    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(d) = kind {
            return d
                .init
                .as_ref()
                .is_some_and(|init| as_expr_is_keyof_array(init, obj_name, aliases));
        }
    }
    false
}

/// True when `init` extracts an element from a `(keyof typeof obj_name)[]` array
/// — via `recv.find(...)`/`.findLast(...)`/`.at(...)` or a computed subscript
/// `recv[i]`. The extracted element is then a known key of `obj_name`.
fn init_yields_obj_key<'a>(
    init: &Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    match init {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(m) = &call.callee
                && matches!(m.property.name.as_str(), "find" | "findLast" | "at")
            {
                return array_elem_keys_obj(&m.object, obj_name, semantic, aliases);
            }
            false
        }
        Expression::ComputedMemberExpression(m) => {
            array_elem_keys_obj(&m.object, obj_name, semantic, aliases)
        }
        _ => false,
    }
}

/// The declared type annotation of `expr`'s binding, when `expr` is an
/// identifier resolving to a formal parameter or a variable declarator that
/// carries an explicit type. `None` for anything un-annotated or unresolved.
fn binding_declared_type<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    let Expression::Identifier(id) = expr else { return None };
    let ref_id = id.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::FormalParameter(param) => {
                return param.type_annotation.as_ref().map(|a| &a.type_annotation);
            }
            AstKind::VariableDeclarator(decl) => {
                return decl.type_annotation.as_ref().map(|a| &a.type_annotation);
            }
            _ => continue,
        }
    }
    None
}

/// True when `ty` is `T[]` (a `TSArrayType`) or a tuple `[T, ...T[]]` (a
/// non-empty `TSTupleType`) whose every element type resolves to `keyof typeof
/// obj_name` (directly or via alias). Iterating such a value yields known keys.
fn array_or_tuple_element_keys_obj(
    ty: &TSType,
    obj_name: &str,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    match ty {
        TSType::TSArrayType(arr) => {
            type_keys_obj(skip_parens(&arr.element_type), obj_name, aliases)
        }
        TSType::TSTupleType(tuple) => {
            !tuple.element_types.is_empty()
                && tuple
                    .element_types
                    .iter()
                    .all(|el| tuple_element_keys_obj(el, obj_name, aliases))
        }
        _ => false,
    }
}

/// True when a single tuple element resolves to `keyof typeof obj_name`. A rest
/// element (`...T[]`) carries an array type, so it recurses; a plain or optional
/// element carries the key type directly.
fn tuple_element_keys_obj(
    el: &TSTupleElement,
    obj_name: &str,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    match el {
        TSTupleElement::TSRestType(rest) => {
            array_or_tuple_element_keys_obj(&rest.type_annotation, obj_name, aliases)
        }
        TSTupleElement::TSOptionalType(opt) => {
            type_keys_obj(skip_parens(&opt.type_annotation), obj_name, aliases)
        }
        other => other
            .as_ts_type()
            .is_some_and(|inner| type_keys_obj(skip_parens(inner), obj_name, aliases)),
    }
}

/// True when the un-annotated formal parameter at `param_node_id` is the first
/// parameter of a callback passed as the first argument to
/// `.map()`/`.forEach()`/`.filter()`/`.some()`/`.every()`, whose array-method
/// receiver is a binding declared `T[]` or `[T, ...T[]]` with `T` resolving to
/// `keyof typeof obj_name`. TypeScript then infers the parameter's type as
/// `keyof typeof obj_name`, so the lookup is as key-narrow-safe as an explicit
/// annotation.
fn param_inferred_from_typed_array_receiver<'a>(
    param_node_id: oxc_semantic::NodeId,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::FormalParameter(param) = nodes.kind(param_node_id) else { return false };
    let param_span = param.span;
    for anc in nodes.ancestors(param_node_id) {
        let params = match anc.kind() {
            AstKind::ArrowFunctionExpression(f) => &f.params,
            AstKind::Function(f) => &f.params,
            _ => continue,
        };
        // Only the callback's first parameter is the element; a later parameter
        // (index, array) is not inferred from the element type.
        if params.items.first().map(|p| p.span) != Some(param_span) {
            return false;
        }
        let AstKind::CallExpression(call) = nodes.parent_kind(anc.id()) else { return false };
        let Expression::StaticMemberExpression(m) = &call.callee else { return false };
        if !matches!(
            m.property.name.as_str(),
            "map" | "forEach" | "filter" | "some" | "every"
        ) {
            return false;
        }
        // The callback must be the first argument (the iteratee), not a thisArg.
        if call.arguments.first().map(|a| a.span()) != Some(anc.kind().span()) {
            return false;
        }
        return binding_declared_type(&m.object, semantic)
            .is_some_and(|ty| array_or_tuple_element_keys_obj(ty, obj_name, aliases));
    }
    false
}

/// True when the index identifier's declared type is `keyof typeof obj_name`
/// (directly or via alias), or a generic type parameter whose constraint
/// resolves to it — the lookup is then statically key-narrow and safe. For an
/// un-annotated variable, also true when its initializer extracts an element
/// from a `(keyof typeof obj_name)[]` array (its elements are known keys). For
/// an un-annotated callback parameter, also true when it is the first parameter
/// of a `.map()`/`.forEach()`/`.filter()`/`.some()`/`.every()` callback whose
/// receiver is declared `T[]` or `[T, ...T[]]` with `T` resolving to `keyof
/// typeof obj_name` (its element type is then a known key).
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
    for node_id in std::iter::once(decl_node_id).chain(nodes.ancestor_ids(decl_node_id)) {
        let ann = match nodes.kind(node_id) {
            AstKind::FormalParameter(param) => {
                // No annotation: accept when the parameter's type is inferred
                // from a typed array receiver of an array-method callback.
                if param.type_annotation.is_none() {
                    return param_inferred_from_typed_array_receiver(
                        node_id, obj_name, semantic, aliases,
                    );
                }
                param.type_annotation.as_ref()
            }
            AstKind::VariableDeclarator(decl) => {
                // No annotation: accept when the initializer extracts an element
                // from a `(keyof typeof Obj)[]` array (its elements are keys).
                if decl.type_annotation.is_none() {
                    return decl.init.as_ref().is_some_and(|init| {
                        init_yields_obj_key(init, obj_name, semantic, aliases)
                    });
                }
                decl.type_annotation.as_ref()
            }
            _ => continue,
        };
        let Some(ann) = ann else { return false };
        if type_keys_obj(&ann.type_annotation, obj_name, aliases) {
            return true;
        }
        // `code: TCode` where `<TCode extends keyof typeof Obj>` is as safe as a
        // direct `keyof typeof Obj` annotation — resolve the constraint.
        return type_ref_name(&ann.type_annotation).is_some_and(|name| {
            type_param_constraint_keys_obj(name, decl_node_id, obj_name, semantic, aliases)
        });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
    fn allows_key_typed_as_generic_param_constrained_by_keyof_typeof_alias() {
        // Regression for issue #556: a generic parameter `TCode extends
        // CurrencyCode` (where `type CurrencyCode = keyof typeof CURRENCIES_MAP`)
        // guarantees the key is valid — same safety as a direct `keyof typeof`.
        let src = "const CURRENCIES_MAP = { USD: 1, EUR: 2 } as const;\n\
                   type CurrencyCode = keyof typeof CURRENCIES_MAP;\n\
                   function currencyFor<TCode extends CurrencyCode>(code: TCode) { return CURRENCIES_MAP[code]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_key_typed_as_generic_param_constrained_directly_by_keyof_typeof() {
        let src = "const CURRENCIES_MAP = { USD: 1, EUR: 2 } as const;\n\
                   function currencyFor<TCode extends keyof typeof CURRENCIES_MAP>(code: TCode) { return CURRENCIES_MAP[code]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_key_typed_as_generic_param_on_arrow_function() {
        let src = "const M = { a: 1, b: 2 } as const;\n\
                   const f = <T extends keyof typeof M>(k: T) => M[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_generic_param_constrained_by_string() {
        let src = "const M = { a: 1, b: 2 } as const;\n\
                   function f<T extends string>(k: T) { return M[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_unconstrained_generic_param() {
        let src = "const M = { a: 1, b: 2 } as const;\n\
                   function f<T>(k: T) { return M[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_generic_param_constrained_by_keyof_typeof_other_object() {
        let src = "const M = { a: 1 } as const;\n\
                   const OTHER = { x: 1 } as const;\n\
                   function f<T extends keyof typeof OTHER>(k: T) { return M[k]; }";
        assert_eq!(run(src).len(), 1);
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

    #[test]
    fn allows_index_from_keyof_cast_array_via_find() {
        // Regression for issue #6676: `keys` is cast `(keyof typeof m)[]`, so the
        // element returned by `.find()` is a known key — the lookup is statically
        // key-narrow, not the widening enum pattern.
        let src = "const m = { a: [1], b: [2] } as const;\n\
                   const keys = Object.keys(m) as (keyof typeof m)[];\n\
                   function g(p: string) {\n\
                   const k = keys.find(x => p.endsWith(x));\n\
                   if (!k) { return; }\n\
                   return m[k];\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_keyof_cast_array_via_subscript() {
        let src = "const m = { a: [1], b: [2] } as const;\n\
                   const keys = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_string_key_with_no_keyof_cast() {
        let src = "const m = { a: 1 } as const;\n\
                   function f(s: string) { const k: string = s; return m[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_non_keyof_array() {
        // `arr` is `string[]` (no `keyof typeof m` cast), so an element pulled out
        // of it is not a known key of `m`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const arr = ['a', 'b'];\n\
                   const k = arr.find(x => x === 'a');\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_callback_param_inferred_from_tuple_rest_receiver() {
        // Regression for issue #7046: the `.map()` callback parameter `type` is
        // inferred as `keyof typeof HASH_LENGTHS` because the receiver `types` is
        // declared `[HashType, ...HashType[]]` and `HashType = keyof typeof
        // HASH_LENGTHS`. The lookup is key-narrow-safe.
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   type HashType = keyof typeof HASH_LENGTHS;\n\
                   function f(types: [HashType, ...HashType[]]) {\n\
                   return types.map((type) => `${HASH_LENGTHS[type]}`);\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_forEach_callback_param_inferred_from_array_receiver() {
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   type HashType = keyof typeof HASH_LENGTHS;\n\
                   function g(types: HashType[]) {\n\
                   types.forEach((type) => { const n = HASH_LENGTHS[type]; });\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_callback_param_inferred_from_keyof_typeof_array_no_alias() {
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   function h(types: (keyof typeof HASH_LENGTHS)[]) {\n\
                   return types.map((k) => HASH_LENGTHS[k]);\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_map_callback_param_from_string_array_receiver() {
        // `types: string[]` — the inferred element type is `string`, not a key.
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   function b(types: string[]) { return types.map((type) => HASH_LENGTHS[type]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_map_callback_param_indexing_different_object() {
        // The receiver keys `HASH_LENGTHS`, but the lookup targets `OTHER` — the
        // element type is not `keyof typeof OTHER`, so it stays unsafe.
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   const OTHER = { x: 1 } as const;\n\
                   type HashType = keyof typeof HASH_LENGTHS;\n\
                   function f(types: HashType[]) { return types.map((type) => OTHER[type]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_first_callback_param_from_typed_receiver() {
        // Indexing with the second (index) parameter `i`, not the element `type`.
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   type HashType = keyof typeof HASH_LENGTHS;\n\
                   function f(types: HashType[]) { return types.map((type, i) => HASH_LENGTHS[i]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_array_method_callback_param_from_typed_receiver() {
        // `.reduce()` is not an element-iterating method: its first callback
        // parameter is the accumulator, not a key of `HASH_LENGTHS`.
        let src = "const HASH_LENGTHS = { md5: 32, sha1: 40 } as const;\n\
                   type HashType = keyof typeof HASH_LENGTHS;\n\
                   function f(types: HashType[]) { return types.reduce((type) => HASH_LENGTHS[type]); }";
        assert_eq!(run(src).len(), 1);
    }
}
