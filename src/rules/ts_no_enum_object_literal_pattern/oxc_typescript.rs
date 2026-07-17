//! ts-no-enum-object-literal-pattern — OXC backend.
//! Flags `Color[someVar]` where `Color` is declared `as const`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, peel_parens};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, CallExpression, Expression, FormalParameter, FormalParameters, FunctionBody,
    IdentifierReference, ObjectExpression, ObjectPropertyKind, PropertyKey, Statement, TSLiteral,
    TSSignature, TSTupleElement, TSType, TSTypeAnnotation, TSTypeOperatorOperator,
    TSTypeParameterDeclaration, TSTypeQueryExprName, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub struct Check;

/// Collect `const X = { ... } as const` bindings as `name -> set of the
/// object's static string keys` (non-computed identifier and string-literal
/// property names).
fn collect_as_const_objects<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> FxHashMap<&'a str, FxHashSet<&'a str>> {
    let mut objects = FxHashMap::default();
    for node in semantic.nodes().iter() {
        let AstKind::VariableDeclaration(decl) = node.kind() else { continue };
        if decl.kind != VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &decl.declarations {
            let Some(init) = &declarator.init else { continue };
            // An explicit type annotation replaces the narrow `as const` literal
            // with the annotated type. Only a closed object-literal annotation
            // (fixed named keys, no index signature) restates the same fixed-key
            // shape the rule targets, so it stays registered. Any other
            // annotation is treated as no longer that pattern: an index signature
            // or mapped type genuinely opens the key space, and a named reference
            // (`Record<K, V>`, an interface, an alias) is left unresolved and
            // conservatively excluded (favouring no false positive).
            if declarator
                .type_annotation
                .as_ref()
                .is_some_and(|ann| !is_closed_object_literal(&ann.type_annotation))
            {
                continue;
            }
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
            let Expression::ObjectExpression(obj) = &as_expr.expression else { continue };
            // Get the binding name.
            if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                objects.insert(id.name.as_str(), object_literal_keys(obj));
            }
        }
    }
    objects
}

/// The set of statically-known string keys of an object literal — the names of
/// its non-computed identifier and string-literal properties. Computed, numeric,
/// and spread properties are omitted, so membership is a sound (not necessarily
/// complete) test that a string is a key of the object.
fn object_literal_keys<'a>(obj: &'a ObjectExpression<'a>) -> FxHashSet<&'a str> {
    let mut keys = FxHashSet::default();
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        if p.computed {
            continue;
        }
        match &p.key {
            PropertyKey::StaticIdentifier(id) => {
                keys.insert(id.name.as_str());
            }
            PropertyKey::StringLiteral(s) => {
                keys.insert(s.value.as_str());
            }
            _ => {}
        }
    }
    keys
}

/// True when `ty` is an object-literal type with a closed set of named keys — a
/// `TSTypeLiteral` carrying no index signature. Such an annotation restates the
/// same fixed-key shape an `as const` object already has, so indexing it with an
/// arbitrary key is still the enum-replacement pattern. Any other annotation is
/// not that pattern: an index signature or mapped type opens the key space, and
/// a named reference (`Record<K, V>`, an interface, an alias) is left unresolved
/// and conservatively excluded.
fn is_closed_object_literal(ty: &TSType) -> bool {
    let TSType::TSTypeLiteral(lit) = ty else { return false };
    !lit.members.iter().any(|m| matches!(m, TSSignature::TSIndexSignature(_)))
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

/// If `ty` is `keyof X` where `X` is a bare type reference (e.g. a generic type
/// parameter), return `X`'s name. Distinct from `keyof_typeof_target`, which
/// handles `keyof typeof X`.
fn keyof_type_param_target<'a>(ty: &'a TSType<'a>) -> Option<&'a str> {
    let TSType::TSTypeOperatorType(op) = ty else { return None };
    if op.operator != TSTypeOperatorOperator::Keyof {
        return None;
    }
    type_ref_name(&op.type_annotation)
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

/// The element type of an array type annotation: `E` from `E[]` (a
/// `TSArrayType`) or from `Array<E>` (a `TSTypeReference` to `Array` with a
/// single type argument). `None` for any other shape.
fn array_type_element<'r, 'a>(ty: &'r TSType<'a>) -> Option<&'r TSType<'a>> {
    match ty {
        TSType::TSArrayType(arr) => Some(skip_parens(&arr.element_type)),
        TSType::TSTypeReference(r) => {
            let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &r.type_name else {
                return None;
            };
            if id.name.as_str() != "Array" {
                return None;
            }
            r.type_arguments.as_ref()?.params.first().map(skip_parens)
        }
        _ => None,
    }
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

/// The declared type of the binding named `binding_name` inside `param`. A plain
/// identifier parameter (`value: T`) carries the type directly; a destructured
/// object parameter (`{ event }: { event: T }`) carries it on the object-type
/// member whose key is `binding_name`. `None` for an un-annotated parameter, a
/// non-object destructuring, or a missing member.
fn param_binding_type<'a>(
    param: &'a FormalParameter<'a>,
    binding_name: &str,
) -> Option<&'a TSType<'a>> {
    let ty = &param.type_annotation.as_ref()?.type_annotation;
    match &param.pattern {
        BindingPattern::BindingIdentifier(_) => Some(ty),
        BindingPattern::ObjectPattern(_) => object_type_member(ty, binding_name),
        _ => None,
    }
}

/// The member type for the non-computed property named `name` in an object-type
/// literal (`{ name: T }`). `None` when `ty` is not a type literal or has no such
/// property.
fn object_type_member<'a>(ty: &'a TSType<'a>, name: &str) -> Option<&'a TSType<'a>> {
    let TSType::TSTypeLiteral(lit) = ty else { return None };
    lit.members.iter().find_map(|m| {
        let TSSignature::TSPropertySignature(prop) = m else { return None };
        if !prop.key.is_specific_static_name(name) {
            return None;
        }
        prop.type_annotation.as_ref().map(|a| &a.type_annotation)
    })
}

/// True when `ty` is a non-empty union of string-literal types (`"a" | "b"`),
/// every member of which is a key of the indexed object (`obj_keys`). A key with
/// such a type ranges only over the object's own keys, so the lookup is
/// statically key-narrow — the same conservatism applied to a `keyof typeof X`
/// key.
fn union_of_literal_keys(ty: &TSType, obj_keys: &FxHashSet<&str>) -> bool {
    let TSType::TSUnionType(union) = ty else { return false };
    !union.types.is_empty() && union.types.iter().all(|m| string_literal_type_is_key(m, obj_keys))
}

/// True when `ty` is a string-literal type (`"a"`) whose value is a key of the
/// indexed object (`obj_keys`).
fn string_literal_type_is_key(ty: &TSType, obj_keys: &FxHashSet<&str>) -> bool {
    let TSType::TSLiteralType(lit) = ty else { return false };
    let TSLiteral::StringLiteral(s) = &lit.literal else { return false };
    obj_keys.contains(s.value.as_str())
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

/// The element type of the array a function/arrow returns: from an explicit
/// `: Array<E>` / `: E[]` return annotation, or from a trailing `... as Array<E>`
/// / `... as E[]` cast in the body — a concise-body arrow's implicit-return
/// expression (`expression_body`) or a `return <expr> as ...` statement.
fn return_array_element_type<'a>(
    return_type: Option<&'a TSTypeAnnotation<'a>>,
    body: Option<&'a FunctionBody<'a>>,
    expression_body: bool,
) -> Option<&'a TSType<'a>> {
    if let Some(rt) = return_type
        && let Some(el) = array_type_element(&rt.type_annotation)
    {
        return Some(el);
    }
    let body = body?;
    for stmt in &body.statements {
        let returned = match stmt {
            Statement::ExpressionStatement(es) if expression_body => &es.expression,
            Statement::ReturnStatement(rs) => match &rs.argument {
                Some(arg) => arg,
                None => continue,
            },
            _ => continue,
        };
        if let Expression::TSAsExpression(as_expr) = peel_parens(returned)
            && let Some(el) = array_type_element(&as_expr.type_annotation)
        {
            return Some(el);
        }
    }
    None
}

/// True when `elem_ty` is `keyof T` with `T` a generic parameter of the callee
/// (`type_params`), and the call argument at the position of the parameter
/// annotated `: T` is the identifier `obj_name`. `T` is then instantiated as
/// `typeof obj_name`, so each returned element is a known key of `obj_name`.
///
/// `T` must be one of the callee's own type parameters: for a concrete `T` the
/// `arr: T` parameter would not bind `T` to the argument's type, so `keyof T`
/// would be unrelated to `obj_name`.
fn call_elem_binds_obj(
    type_params: Option<&TSTypeParameterDeclaration>,
    params: &FormalParameters,
    elem_ty: Option<&TSType>,
    call: &CallExpression,
    obj_name: &str,
) -> bool {
    let Some(elem_ty) = elem_ty else { return false };
    let Some(tp_name) = keyof_type_param_target(elem_ty) else { return false };
    let Some(type_params) = type_params else { return false };
    if !type_params.params.iter().any(|tp| tp.name.name.as_str() == tp_name) {
        return false;
    }
    let Some(arg_index) = params.items.iter().position(|p| {
        p.type_annotation
            .as_ref()
            .is_some_and(|a| type_ref_name(&a.type_annotation) == Some(tp_name))
    }) else {
        return false;
    };
    matches!(
        call.arguments.get(arg_index).and_then(|a| a.as_expression()),
        Some(Expression::Identifier(id)) if id.name.as_str() == obj_name
    )
}

/// True when `call` invokes a generic helper whose declared return element type
/// is `keyof T`, where `T` is bound (through the `arr: T` parameter) to the
/// argument `obj_name`. Each element the call yields is then a known key of
/// `obj_name`, so indexing `obj_name` with such an element is key-narrow-safe.
fn call_yields_obj_keys<'a>(
    call: &'a CallExpression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(callee) = &call.callee else { return false };
    let Some(ref_id) = callee.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        let (type_params, params, elem_ty) = match kind {
            AstKind::Function(f) => (
                f.type_parameters.as_deref(),
                &f.params,
                return_array_element_type(f.return_type.as_deref(), f.body.as_deref(), false),
            ),
            AstKind::VariableDeclarator(d) => match d.init.as_ref() {
                Some(Expression::ArrowFunctionExpression(a)) => (
                    a.type_parameters.as_deref(),
                    &a.params,
                    return_array_element_type(
                        a.return_type.as_deref(),
                        Some(a.body.as_ref()),
                        a.expression,
                    ),
                ),
                Some(Expression::FunctionExpression(f)) => (
                    f.type_parameters.as_deref(),
                    &f.params,
                    return_array_element_type(f.return_type.as_deref(), f.body.as_deref(), false),
                ),
                _ => return false,
            },
            _ => continue,
        };
        return call_elem_binds_obj(type_params, params, elem_ty, call, obj_name);
    }
    false
}

/// True when the array-method receiver `object` yields elements that are known
/// keys of `obj_name`, so the callback's first parameter is inferred as `keyof
/// typeof obj_name`. Three receiver shapes qualify:
///   - an identifier bound to a `T[]` / `[T, ...T[]]` declared type whose element
///     resolves to `keyof typeof obj_name`;
///   - an inline `... as (keyof typeof obj_name)[]` / `... as Array<keyof typeof
///     obj_name>` cast (e.g. `(Object.keys(obj) as (keyof typeof obj)[]).forEach`);
///   - a call to a generic helper returning `Array<keyof T>` / `(keyof T)[]` whose
///     `T`-bound argument is `obj_name` itself (e.g. the `keysOf(obj)` helper).
fn receiver_element_keys_obj<'a>(
    object: &'a Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
) -> bool {
    if binding_declared_type(object, semantic)
        .is_some_and(|ty| array_or_tuple_element_keys_obj(ty, obj_name, aliases))
    {
        return true;
    }
    match peel_parens(object) {
        Expression::TSAsExpression(as_expr) => array_type_element(&as_expr.type_annotation)
            .is_some_and(|el| type_keys_obj(el, obj_name, aliases)),
        Expression::CallExpression(call) => call_yields_obj_keys(call, obj_name, semantic),
        _ => false,
    }
}

/// True when the un-annotated formal parameter at `param_node_id` is the first
/// parameter of a callback passed as the first argument to
/// `.map()`/`.forEach()`/`.filter()`/`.some()`/`.every()`, whose array-method
/// receiver yields elements resolving to `keyof typeof obj_name` (see
/// `receiver_element_keys_obj`). TypeScript then infers the parameter's type as
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
        return receiver_element_keys_obj(&m.object, obj_name, semantic, aliases);
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
/// typeof obj_name` (its element type is then a known key). Also true when the
/// declared type is a string-literal union (`"a" | "b"`) every member of which
/// is a key of the object (`obj_keys`) — it ranges only over the object's keys,
/// so it is as key-narrow as `keyof typeof obj_name`.
fn index_ident_keys_obj<'a>(
    id: &IdentifierReference<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    aliases: &FxHashMap<&str, &str>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for node_id in std::iter::once(decl_node_id).chain(nodes.ancestor_ids(decl_node_id)) {
        let ty = match nodes.kind(node_id) {
            AstKind::FormalParameter(param) => {
                // No annotation: accept when the parameter's type is inferred
                // from a typed array receiver of an array-method callback.
                if param.type_annotation.is_none() {
                    return param_inferred_from_typed_array_receiver(
                        node_id, obj_name, semantic, aliases,
                    );
                }
                let Some(ty) = param_binding_type(param, id.name.as_str()) else {
                    return false;
                };
                ty
            }
            AstKind::VariableDeclarator(decl) => {
                // No annotation: accept when the initializer extracts an element
                // from a `(keyof typeof Obj)[]` array (its elements are keys).
                let Some(ann) = decl.type_annotation.as_ref() else {
                    return decl.init.as_ref().is_some_and(|init| {
                        init_yields_obj_key(init, obj_name, semantic, aliases)
                    });
                };
                &ann.type_annotation
            }
            _ => continue,
        };
        if type_keys_obj(ty, obj_name, aliases) {
            return true;
        }
        // A key whose declared type is a string-literal union drawn entirely from
        // the object's keys (`"a" | "b"`) is as key-narrow as `keyof typeof Obj`.
        if union_of_literal_keys(ty, obj_keys) {
            return true;
        }
        // `code: TCode` where `<TCode extends keyof typeof Obj>` is as safe as a
        // direct `keyof typeof Obj` annotation — resolve the constraint.
        return type_ref_name(ty).is_some_and(|name| {
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

/// True when the index expression is a conditional (`c ? a : b`) or an `||` / `??`
/// logical expression, recursively, whose every leaf operand is a string literal
/// that is a key of the indexed object (`obj_keys`). Such an index has a
/// literal-union type drawn entirely from the object's own keys, so the lookup is
/// statically key-narrow — not a widening arbitrary-key access. `&&` is excluded:
/// its value is not necessarily one of its operands. Recursion descends only into
/// strict sub-expressions of a finite AST, so it terminates.
fn index_is_literal_key_union(expr: &Expression, obj_keys: &FxHashSet<&str>) -> bool {
    match peel_parens(expr) {
        Expression::StringLiteral(s) => obj_keys.contains(s.value.as_str()),
        Expression::ConditionalExpression(cond) => {
            index_is_literal_key_union(&cond.consequent, obj_keys)
                && index_is_literal_key_union(&cond.alternate, obj_keys)
        }
        Expression::LogicalExpression(logic)
            if logic.operator.is_or() || logic.operator.is_coalesce() =>
        {
            index_is_literal_key_union(&logic.left, obj_keys)
                && index_is_literal_key_union(&logic.right, obj_keys)
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

        let objects = collect_as_const_objects(semantic);
        let Some(obj_keys) = objects.get(obj_name) else {
            return;
        };

        // A ternary or `||`/`??` chain whose leaves are all literal keys of the
        // object indexes with a literal-union of the object's own keys — a
        // key-narrow lookup, not a widening arbitrary-key access.
        if index_is_literal_key_union(&member.expression, obj_keys) {
            return;
        }

        // A variable typed `keyof typeof Obj` (directly or via a type alias)
        // makes the lookup statically key-narrow — the canonical, correct way
        // to read an `as const` map. Not the widening enum-replacement pattern.
        if let Expression::Identifier(idx_id) = &member.expression {
            let aliases = collect_keyof_typeof_aliases(semantic);
            if index_ident_keys_obj(idx_id, obj_name, semantic, &aliases, obj_keys) {
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

    #[test]
    fn allows_forEach_over_call_returning_keyof_array_arrow_cast() {
        // Regression for issue #7239: `keysOf` returns `Array<keyof T>` via a
        // trailing `as` cast in its concise body, so `keysOf(states)` has element
        // type `keyof typeof states` and the `.forEach` callback key is a known key.
        let src = "const keysOf = <T extends object>(arr: T) => Object.keys(arr) as Array<keyof T>;\n\
                   const states = { a: 1, b: 2 } as const;\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_forEach_over_call_with_explicit_keyof_array_return_type() {
        // The return element type is read from the explicit `: Array<keyof T>`
        // annotation.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   function keysOf<T extends object>(arr: T): Array<keyof T> { return Object.keys(arr) as Array<keyof T>; }\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_forEach_over_call_returning_keyof_array_block_return_cast() {
        // `(keyof T)[]` element form, read from a `return … as (keyof T)[]` cast
        // in a block body with no return-type annotation.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   function keysOf<T extends object>(arr: T) { return Object.keys(arr) as (keyof T)[]; }\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_forEach_over_call_to_function_expression_helper() {
        // Same resolution for a `const f = function <T>(…)` helper.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   const keysOf = function <T extends object>(arr: T): Array<keyof T> { return Object.keys(arr) as Array<keyof T>; };\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_forEach_over_inline_keyof_typeof_array_cast_receiver() {
        // The receiver is itself an inline `... as (keyof typeof states)[]` cast.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   (Object.keys(states) as (keyof typeof states)[]).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_forEach_over_call_returning_string_array() {
        // `getKeys` returns `string[]` (not `keyof T`), so its elements are
        // arbitrary strings — indexing `states` with one is not key-narrow.
        let src = "const getKeys = <T extends object>(arr: T): string[] => Object.keys(arr) as string[];\n\
                   const states = { a: 1, b: 2 } as const;\n\
                   getKeys(states).forEach((key) => { states[key]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_forEach_over_call_keying_a_different_object() {
        // `keysOf(other)` yields keys of `other`, not `states`, so `states[key]`
        // stays unsafe.
        let src = "const keysOf = <T extends object>(arr: T) => Object.keys(arr) as Array<keyof T>;\n\
                   const states = { a: 1, b: 2 } as const;\n\
                   const other = { c: 3 } as const;\n\
                   keysOf(other).forEach((key) => { states[key]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_record_annotated_as_const_object() {
        // Regression for issue #7531: `grantTypeMap` carries an explicit
        // `Record<GrantTypes, V>` annotation, so its type is the annotation, not
        // the narrow `as const` literal — indexing with a `GrantTypes` key never
        // widens and is not the enum-replacement pattern.
        let src = "type GrantTypes = 'AUTHORIZATION_CODE' | 'CLIENT_CREDENTIALS' | 'IMPLICIT' | 'PASSWORD';\n\
                   const grantTypeMap: Record<GrantTypes, 'authCode' | 'clientCredentials' | 'password' | 'implicit'> = {\n\
                   AUTHORIZATION_CODE: 'authCode',\n\
                   CLIENT_CREDENTIALS: 'clientCredentials',\n\
                   IMPLICIT: 'implicit',\n\
                   PASSWORD: 'password',\n\
                   } as const;\n\
                   function f(currentGrantType: GrantTypes) { return grantTypeMap[currentGrantType]; }\n\
                   function g(key: GrantTypes) { return grantTypeMap[key]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_simple_record_annotated_as_const_object() {
        let src = "const m: Record<string, number> = { a: 1 } as const;\n\
                   function f(k: string) { return m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_signature_annotated_as_const_object() {
        // An index-signature annotation opens the key space, so the binding's
        // type is not the fixed-key `as const` shape — not the enum pattern.
        let src = "const m: { [k: string]: number } = { a: 1 } as const;\n\
                   function f(k: string) { return m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_closed_object_literal_annotation() {
        // A closed object-literal annotation (fixed named keys, no index
        // signature) restates the same narrow shape as the `as const` object, so
        // indexing it with an arbitrary key is still the enum-replacement pattern.
        let src = "const x: { readonly a: 'x'; readonly b: 'y' } = { a: 'x', b: 'y' } as const;\n\
                   function f(k: string) { return x[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_unannotated_as_const_indexed_with_string() {
        // Contrast with the annotated cases: the same object with NO annotation
        // keeps its narrow `as const` type, so an arbitrary-string index widens.
        let src = "const m = { a: 1 } as const;\n\
                   function f(k: string) { return m[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_unannotated_sibling_in_multi_declarator() {
        // The guard skips only the annotated declarator: the `Record`-annotated
        // `a` is not the pattern, but the unannotated `as const` sibling `b` in
        // the same statement still flags.
        let src = "const a: Record<string, number> = { x: 1 } as const, b = { y: 2 } as const;\n\
                   function f(k: string) { return a[k] + b[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ternary_of_literal_keys_index() {
        // Regression for issue #7722(a): `isAnalytics ? "analyticsApi" : "api"`
        // has type `"analyticsApi" | "api"`, both keys of the object — a
        // key-narrow lookup, not a widening arbitrary-key access.
        let src = "const X = { api: { a: 1 }, analyticsApi: { b: 2 } } as const;\n\
                   function f(isAnalytics: boolean) { return X[isAnalytics ? \"analyticsApi\" : \"api\"]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nested_ternary_of_literal_keys_index() {
        // Nested ternary: every leaf (`a`, `b`, `c`) is a key of the object.
        let src = "const X = { a: 1, b: 2, c: 3 } as const;\n\
                   function f(p: number, q: boolean) { return X[p > 0 ? \"a\" : q ? \"b\" : \"c\"]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_logical_coalesce_of_literal_keys_index() {
        // `??` chain whose operands are all literal keys of the object.
        let src = "const X = { api: 1, analyticsApi: 2 } as const;\n\
                   const r = X[\"analyticsApi\" ?? \"api\"];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_ternary_with_non_key_leaf() {
        // One branch (`missingKey`) is not a key of the object, so the index is
        // not provably key-narrow.
        let src = "const X = { api: 1, analyticsApi: 2 } as const;\n\
                   function f(cond: boolean) { return X[cond ? \"api\" : \"missingKey\"]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_ternary_with_non_literal_leaf() {
        // A non-literal branch (`k`) leaves the index type open — not key-narrow.
        let src = "const X = { api: 1, analyticsApi: 2 } as const;\n\
                   function f(cond: boolean, k: string) { return X[cond ? \"api\" : k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_string_literal_union_key_param() {
        // A parameter typed as a string-literal union equal to the object's keys.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f(event: 'approved' | 'rejected') { return m[event]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_string_literal_union_key_destructured_param() {
        // Regression for issue #7722(b): `event` is destructured with declared
        // type `"approved" | "rejected"`, exactly the keys of `eventToColumnMap`.
        let src = "const eventToColumnMap = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f({ event }: { event: 'approved' | 'rejected' }) { return eventToColumnMap[event]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_string_literal_union_key_with_non_key_member() {
        // `deleted` is not a key of the map, so the union is not a subset of the
        // object's keys — the lookup can widen.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f(event: 'approved' | 'deleted') { return m[event]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_string_key_against_literal_union_map() {
        // The union-key fix must not neuter the rule: an arbitrary `string` key
        // against the same map still widens and is still flagged.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f(event: string) { return m[event]; }";
        assert_eq!(run(src).len(), 1);
    }
}
