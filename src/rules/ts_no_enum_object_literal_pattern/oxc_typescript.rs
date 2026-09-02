//! ts-no-enum-object-literal-pattern — OXC backend.
//! Flags `Color[someVar]` where `Color` is declared `as const`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, peel_parens};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrayExpressionElement, BindingPattern, CallExpression, Expression, FormalParameter,
    FormalParameters, FunctionBody, IdentifierReference, ObjectExpression, ObjectPattern,
    ObjectPropertyKind, PropertyKey, Statement, TSAsExpression, TSLiteral, TSSignature,
    TSTupleElement, TSType, TSTypeAnnotation, TSTypeOperatorOperator, TSTypeParameterDeclaration,
    TSTypeQueryExprName, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

/// The static string keys of the `const X = { ... } as const` object `obj_id`
/// names. The reference is resolved through its own symbol, so an object of the
/// same name declared in an unrelated scope cannot answer for it, and only the
/// binding actually indexed is read — the rule runs on every computed member
/// expression of the file, so answering this by scanning the whole file would
/// redo one file-wide walk per lookup.
///
/// `None` when the reference resolves to anything else: a formal parameter, a
/// `let`/`var` binding (reassignable to an object with other keys), a
/// destructuring (which binds a property, not the object), an initializer that
/// is not an object literal under a `const` assertion, or an annotation that
/// replaces that literal's narrow type.
fn as_const_object_keys<'a>(
    obj_id: &IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<FxHashSet<&'a str>> {
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(obj_id.reference_id.get()?).symbol_id()?;
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for node_id in std::iter::once(decl_node_id).chain(nodes.ancestor_ids(decl_node_id)) {
        match nodes.kind(node_id) {
            // A parameter's value comes from the call site, so the walk stops
            // here rather than reading an enclosing binding's initializer as the
            // parameter's — the same node `binding_declared_type` stops at.
            AstKind::FormalParameter(_) => return None,
            AstKind::VariableDeclarator(decl) => {
                let AstKind::VariableDeclaration(parent) = nodes.parent_kind(node_id) else {
                    return None;
                };
                // `let`/`var` can be reassigned to an object with other keys.
                if parent.kind != VariableDeclarationKind::Const {
                    return None;
                }
                // A destructuring binds a property's value, not the object.
                if !matches!(decl.id, BindingPattern::BindingIdentifier(_)) {
                    return None;
                }
                // An explicit type annotation replaces the narrow `as const` literal
                // with the annotated type. Only a closed object-literal annotation
                // (fixed named keys, no index signature) restates the same fixed-key
                // shape the rule targets, so it stays registered. Any other
                // annotation is treated as no longer that pattern: an index signature
                // or mapped type genuinely opens the key space, and a named reference
                // (`Record<K, V>`, an interface, an alias) is left unresolved and
                // conservatively excluded (favouring no false positive).
                if decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| !is_closed_object_literal(&ann.type_annotation))
                {
                    return None;
                }
                // Must be `{ ... } as const` — an object literal under a const
                // assertion.
                let Some(Expression::TSAsExpression(as_expr)) = decl.init.as_ref() else {
                    return None;
                };
                if !is_const_assertion(&as_expr.type_annotation) {
                    return None;
                }
                let Expression::ObjectExpression(obj) = &as_expr.expression else { return None };
                return Some(object_literal_keys(obj));
            }
            _ => continue,
        }
    }
    None
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

/// True when `ty` is the asserted type of an `expr as const` expression. The
/// parser models the `const` assertion as a reference to a type named `const`.
fn is_const_assertion(ty: &TSType) -> bool {
    type_ref_name(ty) == Some("const")
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

/// The right-hand side of the `type A = <type>` declaration that `ty` names.
/// The reference is resolved through its own symbol, so a same-named alias
/// declared in another scope cannot answer for it. `None` when `ty` is not a
/// bare type reference, when the reference carries type arguments — following
/// `KeysOf<T>` would leave `T` unbound, so it names nothing this analysis can
/// read — or when it resolves to anything but a type alias: an interface, an
/// imported type, a generic parameter.
fn alias_target<'r, 'a: 'r>(
    ty: &'r TSType<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'r TSType<'a>> {
    let TSType::TSTypeReference(r) = ty else { return None };
    if r.type_arguments.is_some() {
        return None;
    }
    let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &r.type_name else { return None };
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(id.reference_id.get()?).symbol_id()?;
    let AstKind::TSTypeAliasDeclaration(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return None;
    };
    Some(&decl.type_annotation)
}

/// The number of `type A = B` hops followed when decoding an annotation. A
/// cyclic alias is invalid TypeScript but reachable input, so the walk is bound.
const MAX_ALIAS_HOPS: usize = 8;

/// Follow `type A = B` declarations from `ty` to the type it ultimately names.
/// Returns `ty` unchanged when the chain ends (see `alias_target`), and the
/// partially-resolved type once the hop bound is spent — still a type
/// reference, so the annotation reads as one this analysis cannot decide on and
/// the initializer answers instead.
fn resolve_alias<'r, 'a: 'r>(
    mut ty: &'r TSType<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> &'r TSType<'a> {
    for _ in 0..MAX_ALIAS_HOPS {
        let Some(target) = alias_target(ty, semantic) else { return ty };
        ty = target;
    }
    ty
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

/// True when every value of type `ty` is a statically-known key of `obj_name` —
/// the one question this rule asks of a type, whether that type annotates the
/// index itself, the elements of an array the index is drawn from, or a generic
/// parameter's constraint.
///
/// `keyof typeof Obj` says so outright. A string-literal type says so when the
/// literal is one of the object's own keys (`obj_keys`): `'a'` ranges over
/// nothing else, so it is as key-narrow as `keyof typeof Obj`. A union says so
/// when every member does, which covers `'a' | 'b'` as well as a mix such as
/// `keyof typeof Obj | 'a'`. Aliases are followed first, so `type K = 'a' | 'b'`
/// answers like the union it names.
fn type_is_obj_key<'a>(
    ty: &TSType<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    let ty = peel_type(resolve_alias(ty, semantic));
    if keyof_typeof_target(ty) == Some(obj_name) {
        return true;
    }
    if string_literal_type_value(ty).is_some_and(|value| obj_keys.contains(value)) {
        return true;
    }
    // An empty union states nothing, so it is not evidence of anything.
    let TSType::TSUnionType(union) = ty else { return false };
    !union.types.is_empty()
        && union
            .types
            .iter()
            .all(|member| type_is_obj_key(member, obj_name, semantic, obj_keys))
}

/// If `ty` is a bare type reference to an identifier, return its name.
fn type_ref_name<'a>(ty: &'a TSType<'a>) -> Option<&'a str> {
    let TSType::TSTypeReference(r) = ty else { return None };
    let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &r.type_name else { return None };
    Some(id.name.as_str())
}

/// True when the generic type parameter named `param_name`, declared on the
/// nearest function ancestor of `decl_node_id` that declares it, has a
/// constraint every value of which is a key of `obj_name` (see
/// `type_is_obj_key`).
/// In valid TypeScript that nearest declarer is the function owning the indexed
/// parameter, so an unrelated same-named `T` cannot apply.
fn type_param_constraint_keys_obj<'a>(
    param_name: &str,
    decl_node_id: oxc_semantic::NodeId,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
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
            .is_some_and(|c| type_is_obj_key(c, obj_name, semantic, obj_keys));
    }
    false
}

/// Strip the wrappers that state no type of their own: `TSParenthesizedType`
/// (the parser preserves parentheses, so the element type of `(keyof typeof
/// X)[]` is a parenthesized node around the `keyof typeof` operator) and the
/// label of a named tuple member (`[first: K]` types its element `K`). Every
/// reader of a type goes through this, so the gate (`is_decidable_type`) and
/// the decider (`type_is_obj_key`) look through the same wrappers.
fn peel_type<'r, 'a>(mut ty: &'r TSType<'a>) -> &'r TSType<'a> {
    loop {
        ty = match ty {
            TSType::TSParenthesizedType(p) => &p.type_annotation,
            // A named member whose element is itself a rest or optional element
            // is not a bare type; the tuple-element walk reads that shape.
            TSType::TSNamedTupleMember(m) => match m.element_type.as_ts_type() {
                Some(inner) => inner,
                None => return ty,
            },
            _ => return ty,
        };
    }
}

/// Strip a `readonly` type operator: `T[]` from `readonly T[]`. The operator
/// wraps the array or tuple type, so the element type sits one level deeper.
fn peel_readonly<'r, 'a>(ty: &'r TSType<'a>) -> &'r TSType<'a> {
    match peel_type(ty) {
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            peel_type(&op.type_annotation)
        }
        other => other,
    }
}

/// The element type of an array type annotation: `E` from `E[]` (a
/// `TSArrayType`) or from `Array<E>` / `ReadonlyArray<E>` (a `TSTypeReference`
/// with a single type argument), `readonly` or not. `None` for any other shape.
fn array_type_element<'r, 'a>(ty: &'r TSType<'a>) -> Option<&'r TSType<'a>> {
    match peel_readonly(ty) {
        TSType::TSArrayType(arr) => Some(peel_type(&arr.element_type)),
        TSType::TSTypeReference(r) => {
            let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &r.type_name else {
                return None;
            };
            if !matches!(id.name.as_str(), "Array" | "ReadonlyArray") {
                return None;
            }
            r.type_arguments.as_ref()?.params.first().map(peel_type)
        }
        _ => None,
    }
}

/// True when `as_expr` asserts an array literal `as const` and every element is a
/// string literal that is a static key of the indexed object (`obj_keys`). The
/// assertion types the literal as a readonly tuple, so its element type is the
/// union of those literals — each element is a known key. An empty literal states
/// nothing, so it does not qualify.
fn as_const_array_of_obj_keys(as_expr: &TSAsExpression, obj_keys: &FxHashSet<&str>) -> bool {
    if !is_const_assertion(&as_expr.type_annotation) {
        return false;
    }
    let Expression::ArrayExpression(arr) = &as_expr.expression else { return false };
    !arr.elements.is_empty()
        && arr.elements.iter().all(|element| {
            let ArrayExpressionElement::StringLiteral(s) = element else { return false };
            obj_keys.contains(s.value.as_str())
        })
}

/// True when `expr` is a cast that states an array element type resolving to
/// `keyof typeof obj_name` (`… as (keyof typeof m)[]`, `… as Array<keyof typeof
/// m>`). The cast names the element type outright, which is what makes it
/// evidence an annotation cannot silently take back.
fn as_expr_states_key_elements<'a>(
    expr: &Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    let Expression::TSAsExpression(as_expr) = expr else { return false };
    array_type_element(&as_expr.type_annotation)
        .is_some_and(|el| type_is_obj_key(el, obj_name, semantic, obj_keys))
}

/// True when `expr` is an `as` expression yielding an array whose elements are
/// statically known keys of `obj_name`. Two shapes qualify: a cast stating the
/// element type (see `as_expr_states_key_elements`), and an `as const` array
/// literal of the object's own keys (see `as_const_array_of_obj_keys`).
fn as_expr_yields_obj_keys<'a>(
    expr: &Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    let Expression::TSAsExpression(as_expr) = expr else { return false };
    as_expr_states_key_elements(expr, obj_name, semantic, obj_keys)
        || as_const_array_of_obj_keys(as_expr, obj_keys)
}

/// The initializer of `expr`'s binding, when `expr` is an identifier resolving
/// to a variable declarator that has one. `None` when it resolves to a formal
/// parameter: a parameter's value comes from the call site, never from a
/// declarator further out, so the walk stops there rather than reading an
/// enclosing binding's initializer as the parameter's. `binding_declared_type`
/// stops at the same node, so the two walks over this chain agree on which node
/// declares the binding.
fn binding_initializer<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    let Expression::Identifier(id) = expr else { return None };
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(id.reference_id.get()?).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::VariableDeclarator(d) => return d.init.as_ref(),
            AstKind::FormalParameter(_) => return None,
            _ => continue,
        }
    }
    None
}

/// True when `ty` names a type the decider (`type_is_obj_key`) can rule on: a
/// keyword (`string`, `any`, …), `keyof typeof X`, a string-literal type, or
/// a union whose every member is one of those. Deliberately nothing else: for a
/// name this analysis could not resolve, or an operator it does not read, the
/// decider answers "not a key" for lack of information, and reading that as a
/// veto would be a verdict it cannot stand behind.
///
/// The gate and the decider must stay in lockstep — same aliases resolved, same
/// wrappers peeled, a union walked member by member by both — or an element type
/// the decider now reads would stay behind a gate that no longer matches it.
fn is_decidable_type<'a>(ty: &TSType<'a>, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    let ty = peel_type(resolve_alias(ty, semantic));
    if keyof_typeof_target(ty).is_some()
        || ty.is_keyword()
        || string_literal_type_value(ty).is_some()
    {
        return true;
    }
    let TSType::TSUnionType(union) = ty else { return false };
    !union.types.is_empty() && union.types.iter().all(|member| is_decidable_type(member, semantic))
}

/// True when `ty` decides on its own what a value of that type holds. That
/// holds for an array whose element type is decidable (see `is_decidable_type`),
/// for a tuple whose every element type is, and for a decidable type that is
/// neither — `string`, `any`: it is plainly not an array of keys. It does not
/// hold for a name this analysis could not resolve, nor for an array or tuple
/// built on one: those state nothing it can act on.
fn annotation_decides_elements<'a>(
    ty: &TSType<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let ty = resolve_alias(ty, semantic);
    if let Some(element) = array_type_element(ty) {
        return is_decidable_type(element, semantic);
    }
    match peel_readonly(ty) {
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .all(|el| tuple_element_decidable(el, semantic)),
        other => is_decidable_type(other, semantic),
    }
}

/// True when a single tuple element states a type the decider can rule on. A
/// rest element (`...T[]`) carries an array type, so it recurses; a plain or
/// optional element carries the element type directly. Mirrors
/// `tuple_element_keys_obj`, which reads what these elements say.
fn tuple_element_decidable<'a>(
    el: &TSTupleElement<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match el {
        TSTupleElement::TSRestType(rest) => {
            annotation_decides_elements(&rest.type_annotation, semantic)
        }
        TSTupleElement::TSOptionalType(opt) => is_decidable_type(&opt.type_annotation, semantic),
        other => other.as_ts_type().is_some_and(|inner| is_decidable_type(inner, semantic)),
    }
}

/// True when `expr` evaluates to an array whose elements are statically known
/// keys of `obj_name`, so a value taken out of it is itself a known key.
///
/// A call to a generic helper whose declared return element type is `keyof T`,
/// with `T` bound to the indexed object, qualifies (see `call_yields_obj_keys`)
/// — the `keysOf(obj)` shape.
///
/// An `as` expression yielding an array of keys qualifies (see
/// `as_expr_yields_obj_keys`), inline or through the initializer of the
/// identifier's binding — a single hop.
///
/// An explicit annotation on that binding is the binding's type, so whenever it
/// decides what the elements are (see `annotation_decides_elements`), it decides
/// alone: `const k: readonly string[] = […] as const` holds arbitrary strings.
/// When it does not, the binding can still hold an array whose element type a
/// cast named outright, and that cast is the rule's own remediation — flagging
/// code that applied it would be unactionable. A bare `as const` literal is not
/// evidence there: it asserts only that its elements are literals, which is what
/// an annotation overrides.
fn expr_yields_obj_key_array<'a>(
    expr: &'a Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    let expr = peel_parens(expr);
    if let Expression::CallExpression(call) = expr {
        return call_yields_obj_keys(call, obj_name, semantic);
    }
    if let Some(ty) = binding_declared_type(expr, semantic) {
        if annotation_decides_elements(ty, semantic) {
            return declared_element_keys_obj(ty, obj_name, semantic, obj_keys);
        }
        return binding_initializer(expr, semantic)
            .is_some_and(|init| as_expr_states_key_elements(init, obj_name, semantic, obj_keys));
    }
    as_expr_yields_obj_keys(expr, obj_name, semantic, obj_keys)
        || binding_initializer(expr, semantic)
            .is_some_and(|init| as_expr_yields_obj_keys(init, obj_name, semantic, obj_keys))
}

/// True when `init` extracts an element from an array of known keys of `obj_name`
/// — via `recv.find(...)`/`.findLast(...)`/`.at(...)` or a computed subscript
/// `recv[i]`. The extracted element is then a known key of `obj_name`.
fn init_yields_obj_key<'a>(
    init: &Expression<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    match init {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(m) = &call.callee
                && matches!(m.property.name.as_str(), "find" | "findLast" | "at")
            {
                return expr_yields_obj_key_array(&m.object, obj_name, semantic, obj_keys);
            }
            false
        }
        Expression::ComputedMemberExpression(m) => {
            expr_yields_obj_key_array(&m.object, obj_name, semantic, obj_keys)
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
/// member whose key is the one `binding_name` is destructured from — which is not
/// the binding's own name under a rename (`{ event: e }` binds `e` from key
/// `event`). `None` for an un-annotated parameter, a non-object destructuring, or
/// a missing member.
fn param_binding_type<'a>(
    param: &'a FormalParameter<'a>,
    binding_name: &str,
) -> Option<&'a TSType<'a>> {
    let ty = &param.type_annotation.as_ref()?.type_annotation;
    match &param.pattern {
        BindingPattern::BindingIdentifier(_) => Some(ty),
        BindingPattern::ObjectPattern(pat) => {
            object_type_member(ty, binding_property_key(pat, binding_name)?)
        }
        _ => None,
    }
}

/// The statically-known property key that `binding_name` is destructured from in
/// `pat` — `event` for both `{ event }` and `{ event: e }` (binding `e`), and for
/// a defaulted `{ event = "approved" }`. `None` when no property with a static,
/// non-computed key binds that name directly; a nested pattern (`{ a: { b } }`)
/// is not resolved.
fn binding_property_key<'a>(pat: &'a ObjectPattern<'a>, binding_name: &str) -> Option<&'a str> {
    pat.properties.iter().find_map(|p| {
        if p.computed || binding_pattern_name(&p.value) != Some(binding_name) {
            return None;
        }
        match &p.key {
            PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
            PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
            _ => None,
        }
    })
}

/// The name a binding pattern binds directly, looking through a default value
/// (`event = "approved"` binds `event`). `None` for a nested destructuring.
fn binding_pattern_name<'a>(pat: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pat {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        BindingPattern::AssignmentPattern(assign) => binding_pattern_name(&assign.left),
        _ => None,
    }
}

/// The member type for the statically-named property `name` in an object-type
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

/// The value of a string-literal type (`"a"`); `None` for any other type. Read
/// by the decider to test key membership and by the gate to recognise the shape.
fn string_literal_type_value<'r, 'a>(ty: &'r TSType<'a>) -> Option<&'r str> {
    let TSType::TSLiteralType(lit) = ty else { return None };
    let TSLiteral::StringLiteral(s) = &lit.literal else { return None };
    Some(s.value.as_str())
}

/// True when `ty` states an element type every value of which is a key of
/// `obj_name` (see `type_is_obj_key`): an array (`T[]`, `Array<T>`, `readonly`
/// or not) or a non-empty tuple `[T, ...T[]]` whose every element does.
/// Iterating such a value yields known keys. Aliases are followed first, so
/// `type Keys = (keyof typeof Obj)[]` states what it names. Anything else —
/// `string[]`, an interface, `any`, an imported type — does not state key
/// elements and so does not qualify.
fn declared_element_keys_obj<'a>(
    ty: &TSType<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    let ty = resolve_alias(ty, semantic);
    if let Some(element) = array_type_element(ty) {
        return type_is_obj_key(element, obj_name, semantic, obj_keys);
    }
    let TSType::TSTupleType(tuple) = peel_readonly(ty) else { return false };
    !tuple.element_types.is_empty()
        && tuple
            .element_types
            .iter()
            .all(|el| tuple_element_keys_obj(el, obj_name, semantic, obj_keys))
}

/// True when a single tuple element states a type every value of which is a key
/// of `obj_name`. A rest element (`...T[]`) carries an array type, so it
/// recurses; a plain or optional element carries the element type directly.
fn tuple_element_keys_obj<'a>(
    el: &TSTupleElement<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    match el {
        TSTupleElement::TSRestType(rest) => {
            declared_element_keys_obj(&rest.type_annotation, obj_name, semantic, obj_keys)
        }
        TSTupleElement::TSOptionalType(opt) => {
            type_is_obj_key(&opt.type_annotation, obj_name, semantic, obj_keys)
        }
        other => other
            .as_ts_type()
            .is_some_and(|inner| type_is_obj_key(inner, obj_name, semantic, obj_keys)),
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

/// True when the un-annotated formal parameter at `param_node_id` is the first
/// parameter of a callback passed as the first argument to
/// `.map()`/`.forEach()`/`.filter()`/`.some()`/`.every()`, whose array-method
/// receiver yields elements that are known keys of `obj_name`. TypeScript then
/// infers the parameter's type as a subtype of `keyof typeof obj_name`, so the
/// lookup is as key-narrow-safe as an explicit annotation. Which receivers
/// yield known keys is `expr_yields_obj_key_array`'s question, not this one's.
fn param_inferred_from_key_array_receiver<'a>(
    param_node_id: oxc_semantic::NodeId,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
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
        return expr_yields_obj_key_array(&m.object, obj_name, semantic, obj_keys);
    }
    false
}

/// True when `decl` is the binding of a `for (const k of <receiver>)` loop
/// whose receiver yields an array of known keys of `obj_name`: `k` then holds
/// one of those elements, exactly as the parameter of a `.forEach` callback
/// over the same receiver does.
///
/// A for-of binding is a declarator with no initializer, so its type is stated
/// by the statement it belongs to, two hops up: the declaration, then the loop.
/// The hops are counted rather than searched — an enclosing for-of must not
/// answer for a binding it does not bind, such as a `let` declared in the loop
/// body or a nested `for...in` key.
fn for_of_binding_keys_obj<'a>(
    decl_node_id: oxc_semantic::NodeId,
    decl: &oxc_ast::ast::VariableDeclarator<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    obj_keys: &FxHashSet<&str>,
) -> bool {
    // Only a plain `const k` binds the element itself; `for (const [k] of ...)`
    // destructures it, and destructuring a key string yields a character.
    if !matches!(decl.id, BindingPattern::BindingIdentifier(_)) {
        return false;
    }
    let nodes = semantic.nodes();
    let declaration_id = nodes.parent_id(decl_node_id);
    if !matches!(nodes.kind(declaration_id), AstKind::VariableDeclaration(_)) {
        return false;
    }
    let AstKind::ForOfStatement(for_of) = nodes.parent_kind(declaration_id) else {
        return false;
    };
    expr_yields_obj_key_array(&for_of.right, obj_name, semantic, obj_keys)
}

/// True when the index identifier's declared type ranges only over keys of
/// `obj_name` (see `type_is_obj_key`), or is a generic type parameter whose
/// constraint does — the lookup is then statically key-narrow and safe.
///
/// An un-annotated binding takes its type from where its value comes from: the
/// initializer, when it extracts an element from an array of known keys (see
/// `init_yields_obj_key`); the loop receiver, for a `for...of` binding (see
/// `for_of_binding_keys_obj`); the receiver of the array method, for the first
/// parameter of a `.map()`/`.forEach()`/`.filter()`/`.some()`/`.every()`
/// callback (see `param_inferred_from_key_array_receiver`).
fn index_ident_keys_obj<'a>(
    id: &IdentifierReference<'a>,
    obj_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
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
                // from an array-method receiver that yields known keys.
                if param.type_annotation.is_none() {
                    return param_inferred_from_key_array_receiver(
                        node_id, obj_name, semantic, obj_keys,
                    );
                }
                let Some(ty) = param_binding_type(param, id.name.as_str()) else {
                    return false;
                };
                ty
            }
            AstKind::VariableDeclarator(decl) => {
                let Some(ann) = decl.type_annotation.as_ref() else {
                    // No annotation and no initializer: a `for (const k of ...)`
                    // binding takes its type from the loop's receiver.
                    let Some(init) = decl.init.as_ref() else {
                        return for_of_binding_keys_obj(
                            node_id, decl, obj_name, semantic, obj_keys,
                        );
                    };
                    // Otherwise accept when the initializer extracts an element
                    // from an array of known keys of the object.
                    return init_yields_obj_key(init, obj_name, semantic, obj_keys);
                };
                &ann.type_annotation
            }
            _ => continue,
        };
        if type_is_obj_key(ty, obj_name, semantic, obj_keys) {
            return true;
        }
        // `code: TCode` where `<TCode extends keyof typeof Obj>` is as safe as a
        // direct `keyof typeof Obj` annotation — resolve the constraint.
        return type_ref_name(ty).is_some_and(|name| {
            type_param_constraint_keys_obj(name, decl_node_id, obj_name, semantic, obj_keys)
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

        let Some(obj_keys) = as_const_object_keys(obj_id, semantic) else {
            return;
        };

        // A ternary or `||`/`??` chain whose leaves are all literal keys of the
        // object indexes with a literal-union of the object's own keys — a
        // key-narrow lookup, not a widening arbitrary-key access.
        if index_is_literal_key_union(&member.expression, &obj_keys) {
            return;
        }

        // A variable typed `keyof typeof Obj` (directly or via a type alias)
        // makes the lookup statically key-narrow — the canonical, correct way
        // to read an `as const` map. Not the widening enum-replacement pattern.
        if let Expression::Identifier(idx_id) = &member.expression
            && index_ident_keys_obj(idx_id, obj_name, semantic, &obj_keys)
        {
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
    fn allows_for_each_callback_param_inferred_from_array_receiver() {
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
    fn allows_for_each_over_call_returning_keyof_array_arrow_cast() {
        // Regression for issue #7239: `keysOf` returns `Array<keyof T>` via a
        // trailing `as` cast in its concise body, so `keysOf(states)` has element
        // type `keyof typeof states` and the `.forEach` callback key is a known key.
        let src = "const keysOf = <T extends object>(arr: T) => Object.keys(arr) as Array<keyof T>;\n\
                   const states = { a: 1, b: 2 } as const;\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_call_with_explicit_keyof_array_return_type() {
        // The return element type is read from the explicit `: Array<keyof T>`
        // annotation.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   function keysOf<T extends object>(arr: T): Array<keyof T> { return Object.keys(arr) as Array<keyof T>; }\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_call_returning_keyof_array_block_return_cast() {
        // `(keyof T)[]` element form, read from a `return … as (keyof T)[]` cast
        // in a block body with no return-type annotation.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   function keysOf<T extends object>(arr: T) { return Object.keys(arr) as (keyof T)[]; }\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_call_to_function_expression_helper() {
        // Same resolution for a `const f = function <T>(…)` helper.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   const keysOf = function <T extends object>(arr: T): Array<keyof T> { return Object.keys(arr) as Array<keyof T>; };\n\
                   keysOf(states).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_inline_keyof_typeof_array_cast_receiver() {
        // The receiver is itself an inline `... as (keyof typeof states)[]` cast.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   (Object.keys(states) as (keyof typeof states)[]).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_inline_as_const_key_literal_array_receiver() {
        // Regression for issue #6863: the receiver `(['hour', 'minute', 'second']
        // as const)` is a readonly tuple of the object's own keys, so the callback
        // parameter is inferred as `'hour' | 'minute' | 'second'` — a subtype of
        // `keyof typeof availableTimeGetters`. The lookup is key-narrow-safe.
        let src = "const availableTimeGetters = {\n\
                   hour: getAvailableHours,\n\
                   minute: getAvailableMinutes,\n\
                   second: getAvailableSeconds,\n\
                   } as const;\n\
                   (['hour', 'minute', 'second'] as const).forEach((type) => {\n\
                   if (availableTimeGetters[type]) {\n\
                   const method = availableTimeGetters[type];\n\
                   }\n\
                   });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_over_as_const_key_literal_array_bound_to_a_const() {
        // The same tuple reached through its binding: the initializer, not the
        // spelling at the call site, carries the element type.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   const values = KEYS.map((k) => m[k]);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_as_const_key_literal_array_subscript() {
        // An element pulled out of the same tuple by subscript is a known key too.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   const k = KEYS[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_for_each_over_literal_array_with_a_non_key_element() {
        // `'missing'` is not a key of the object, so the inferred element type is
        // not a subtype of `keyof typeof m` — the lookup is not key-narrow.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   (['a', 'missing'] as const).forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_literal_array_without_as_const() {
        // Without the assertion the literal widens to `string[]`, so the callback
        // parameter is `string` — an arbitrary key.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   ['a', 'b'].forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_as_const_key_array_widened_by_an_annotation() {
        // The `readonly string[]` annotation is the binding's type and erases the
        // narrow tuple the assertion would have given it: the callback parameter
        // is `string`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS: readonly string[] = ['a', 'b'] as const;\n\
                   KEYS.forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_subscript_of_as_const_key_array_widened_by_an_annotation() {
        // The annotation decides in the subscript path too, not only under an
        // array-method callback: `k` is `string`, so the lookup widens.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS: readonly string[] = ['a', 'b'] as const;\n\
                   const k = KEYS[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_parenthesized_annotated_key_array() {
        // Parentheses around the receiver must not hide the annotation that
        // widens its element type.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS: readonly string[] = ['a', 'b'] as const;\n\
                   (KEYS).forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_index_from_as_const_key_array_via_find() {
        // `.find()` on the same tuple yields `'a' | 'b' | undefined`; the defined
        // half is a known key.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   const k = KEYS.find((x) => x === 'a');\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_array_generic_keyof_cast_via_subscript() {
        // `Array<keyof typeof m>` states the same element type as `(keyof typeof
        // m)[]` in the subscript path as well.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys = Object.keys(m) as Array<keyof typeof m>;\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_as_const_array_of_a_strict_subset_of_the_keys() {
        // The literals need only be keys of the object, not all of them: `'a'` is
        // still a subtype of `keyof typeof m`.
        let src = "const m = { a: 1, b: 2, c: 3 } as const;\n\
                   (['a'] as const).forEach((k) => { const v = m[k]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_for_each_over_as_const_array_with_a_spread_element() {
        // A spread hides which literals the tuple holds, so the element type is
        // not statically known to be a key.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const REST = ['b'] as const;\n\
                   (['a', ...REST] as const).forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_for_each_over_readonly_keyof_annotated_array_param() {
        // `readonly (keyof typeof m)[]` states the same element type as `(keyof
        // typeof m)[]`; the `readonly` operator does not erase it.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   function f(keys: readonly (keyof typeof m)[]) {\n\
                   keys.forEach((k) => { const v = m[k]; });\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_over_readonly_array_generic_keyof_annotated_param() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   function f(keys: ReadonlyArray<keyof typeof m>) {\n\
                   return keys.map((k) => m[k]);\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_map_over_readonly_string_array_annotated_param() {
        // The `readonly` operator is peeled, but `string` is still not a key.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   function f(keys: readonly string[]) { return keys.map((k) => m[k]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_index_from_key_array_annotated_by_a_key_array_alias() {
        // The alias is followed to the array type it names, whose element type
        // is a key.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type Keys = (keyof typeof m)[];\n\
                   const keys: Keys = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_key_array_annotated_by_a_chain_of_aliases() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type Inner = (keyof typeof m)[];\n\
                   type Keys = Inner;\n\
                   const keys: Keys = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_index_from_key_array_annotated_by_a_widening_alias() {
        // The alias names `readonly string[]`, which is the binding's type and
        // erases the narrower tuple its `as const` initializer would have given
        // it: `k` is `string`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type Loose = readonly string[];\n\
                   const keys: Loose = ['a', 'b'] as const;\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_key_array_annotated_by_a_widening_alias() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type Loose = readonly string[];\n\
                   const keys: Loose = ['a', 'b'] as const;\n\
                   keys.forEach((x) => { const v = m[x]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_as_const_key_array_annotated_any() {
        // `any` is the binding's type and states no element type at all.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: any = ['a', 'b'] as const;\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_as_const_key_array_annotated_by_an_interface() {
        // An interface is not followed, so it does not state key elements.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   interface Bag { length: number }\n\
                   const keys: Bag = ['a', 'b'] as const;\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_untyped_param_inside_a_cast_declarator() {
        // `keys` is an un-annotated callback parameter, not the `X` binding whose
        // initializer carries the cast — the walk must stop at the parameter.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   declare function foo(cb: unknown): unknown;\n\
                   const X = foo((keys) => {\n\
                   const k = keys[0];\n\
                   return m[k];\n\
                   }) as (keyof typeof m)[];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_index_from_keyof_cast_annotated_by_an_unresolvable_type() {
        // The annotation names an interface, which states no element type, but the
        // initializer's cast names it — and that cast is what the rule asks for.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   interface Bag { length: number }\n\
                   const keys: Bag = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_keyof_cast_annotated_by_a_generic_alias() {
        // A `KeysOf<T>` helper alias is followed nowhere — its parameter would be
        // unbound — so the initializer's cast decides.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type KeysOf<T> = (keyof T)[];\n\
                   const keys: KeysOf<typeof m> = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_keyof_cast_annotated_by_an_indexed_access_element() {
        // `(typeof KEYS)[number][]` is an array whose element type this analysis
        // does not evaluate; the cast on the initializer states it instead.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   const keys: (typeof KEYS)[number][] = Object.keys(m) as (keyof typeof m)[];\n\
                   keys.forEach((k) => { const v = m[k]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_index_from_as_const_array_annotated_by_an_unresolvable_type() {
        // An `as const` literal asserts only that its elements are literals, so it
        // is not evidence against an annotation that states something else.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   interface Bag { length: number }\n\
                   const keys: Bag = ['a', 'b'] as const;\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_index_from_keyof_cast_annotated_by_a_union_element_array() {
        // Every member of the union is a key of `m` — `keyof typeof m` outright,
        // `'a'` by literal — so the annotation itself states key elements.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: (keyof typeof m | 'a')[] = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_keyof_cast_annotated_by_a_tuple_of_unresolvable_types() {
        // The tuple arm is gated like the array arm: an element the analysis
        // cannot read leaves the tuple unable to decide.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   interface Bag { length: number }\n\
                   const keys: [Bag, Bag] = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_index_from_keyof_cast_annotated_by_a_widening_union_element() {
        // The annotation is the binding's type and `string | number` genuinely
        // widens: every member is a type the decider reads, and none of them is
        // a key, so the annotation vetoes rather than deferring to the cast.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: (string | number)[] = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_index_from_keyof_cast_annotated_by_a_string_literal_element() {
        // `'a'` is a key of `m`, so the annotation states key elements on its
        // own — the cast beside it only restates them.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: 'a'[] = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_index_from_keyof_cast_annotated_by_a_non_key_literal_element() {
        // The mirror direction: `'zzz'` is read just as well, and it is not a
        // key of `m`, so the annotation vetoes.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: 'zzz'[] = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_keyof_cast_annotated_any() {
        // `any` is decidable and is plainly not an array of keys, so it decides
        // and vetoes rather than deferring to the cast.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: any = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_keyof_cast_widened_by_a_readable_annotation() {
        // `readonly string[]` states an element type the decoder reads, and it
        // says `string`. The annotation is the binding's type, so it wins over
        // the cast in the initializer.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: readonly string[] = Object.keys(m) as (keyof typeof m)[];\n\
                   keys.forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_keyof_cast_widened_by_a_readable_annotation() {
        // The same on the subscript path, so the two paths agree.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const keys: string[] = Object.keys(m) as (keyof typeof m)[];\n\
                   const k = keys[0];\n\
                   const v = m[k];";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_key_param_typed_by_an_alias_shadowed_in_an_unrelated_scope() {
        // `K` in `f` is the module-scope alias. A same-named alias declared inside
        // `g` is a different symbol and must not answer for it.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type K = keyof typeof m;\n\
                   function f(k: K) { return m[k]; }\n\
                   function g() { type K = string; const z: K = 'q'; return z; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_key_param_typed_by_a_widening_alias_shadowed_in_an_unrelated_scope() {
        // The mirror direction: `K` in `h` is the module-scope `string`, and the
        // narrow alias inside `i` does not reach it.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type K = string;\n\
                   function h(k: K) { return m[k]; }\n\
                   function i() { type K = keyof typeof m; const z: K = 'a'; return z; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_as_const_key_array_of_another_object() {
        // The tuple holds keys of `other`, not of `m`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const other = { c: 3 } as const;\n\
                   const KEYS = ['c'] as const;\n\
                   KEYS.forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_empty_as_const_array() {
        // An empty literal states nothing about the element type.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   ([] as const).forEach((k) => { const v = m[k]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_for_each_over_inline_keyof_typeof_array_cast_in_array_generic_form() {
        // `Array<keyof typeof states>` states the same element type as `(keyof
        // typeof states)[]`.
        let src = "const states = { a: 1, b: 2 } as const;\n\
                   (Object.keys(states) as Array<keyof typeof states>).forEach((key) => { states[key]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_for_each_over_call_returning_string_array() {
        // `getKeys` returns `string[]` (not `keyof T`), so its elements are
        // arbitrary strings — indexing `states` with one is not key-narrow.
        let src = "const getKeys = <T extends object>(arr: T): string[] => Object.keys(arr) as string[];\n\
                   const states = { a: 1, b: 2 } as const;\n\
                   getKeys(states).forEach((key) => { states[key]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_call_keying_a_different_object() {
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
    fn allows_string_literal_union_key_renamed_destructured_param() {
        // The binding is renamed: `e` is destructured from key `event`, so the
        // member type must be resolved through the pattern's key, not the
        // binding's own name.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f({ event: e }: { event: 'approved' | 'rejected' }) { return m[e]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_string_literal_union_key_defaulted_destructured_param() {
        // A default value does not change which key the binding comes from.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f({ event = 'approved' }: { event?: 'approved' | 'rejected' }) { return m[event]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_renamed_destructured_key_colliding_with_another_member() {
        // `approved` is destructured from key `status` (declared `string`), so it
        // is an arbitrary key. Resolving the member by the binding's own name
        // would wrongly pick up the unrelated `approved` member's literal union
        // and suppress this true positive.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   function f({ status: approved }: { status: string; approved: 'approved' | 'rejected' }) { return m[approved]; }";
        assert_eq!(run(src).len(), 1);
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

    // --- #8353: `for (const k of <key array>)` binds the same element type an
    // --- array-method callback parameter does.

    #[test]
    fn allows_index_from_for_of_over_as_const_key_array() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   for (const k of ['a', 'b'] as const) { const v = m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_for_of_over_identifier_bound_to_as_const_key_array() {
        // The spelling users actually write: the tuple is bound to a `const`
        // first, so the receiver decodes through the binding's initializer.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   for (const k of KEYS) { const v = m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_for_of_over_keyof_cast_array() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   for (const k of Object.keys(m) as (keyof typeof m)[]) { const v = m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_from_for_of_over_call_returning_keyof_array() {
        // The receiver ladder is shared with the callback path, so the
        // `keysOf(obj)` helper answers for a for-of too.
        let src = "const keysOf = <T extends object>(arr: T) => Object.keys(arr) as Array<keyof T>;\n\
                   const m = { a: 1, b: 2 } as const;\n\
                   for (const k of keysOf(m)) { const v = m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_index_from_for_of_over_string_array() {
        // No `as const`, so the literal's element type is `string`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   for (const k of ['a', 'b']) { const v = m[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_of_over_key_array_widened_by_an_annotation() {
        // The annotation is the binding's type and it says `string`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS: readonly string[] = ['a', 'b'] as const;\n\
                   for (const k of KEYS) { const v = m[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_index_from_for_of_destructured_binding() {
        // The receiver decodes, but `k` destructures the first character of the
        // key string, which is `string`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   for (const [k] of ['a', 'b'] as const) { const v = m[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_uninitialized_let_declared_inside_a_for_of_body() {
        // `z` is declared by the body's block, not by the loop; an enclosing
        // for-of must not answer for a binding it does not bind.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   for (const x of KEYS) { let z; z = 'zzz'; const v = m[z]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_in_nested_inside_a_for_of() {
        // A `for...in` key is a `string`; being nested in a for-of over a key
        // array changes nothing.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const KEYS = ['a', 'b'] as const;\n\
                   for (const x of KEYS) { for (const q in m) { const v = m[q]; } }";
        assert_eq!(run(src).len(), 1);
    }

    // --- #8352: an element type that is a string-literal union or a literal
    // --- tuple states keys just as `keyof typeof Obj` does.

    #[test]
    fn allows_index_from_literal_union_element_array() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const u: ('a' | 'b')[] = ['a', 'b'];\n\
                   const k = u[0];\n\
                   const v = m[k];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_literal_union_element_array() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const u: ('a' | 'b')[] = ['a', 'b'];\n\
                   u.forEach((x) => { const v = m[x]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_literal_tuple_of_keys() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const t: readonly ['a', 'b'] = ['a', 'b'];\n\
                   t.forEach((x) => { const v = m[x]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_array_annotated_by_a_literal_union_alias() {
        // The alias is resolved on the element, not only on the whole
        // annotation, so `K[]` reads like `('a' | 'b')[]`.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   type K = 'a' | 'b';\n\
                   const w: K[] = ['a', 'b'];\n\
                   w.forEach((x) => { const v = m[x]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_each_over_literal_union_element_cast() {
        // The cast spelling and the annotation spelling must agree.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const u = ['a', 'b'] as ('a' | 'b')[];\n\
                   u.forEach((x) => { const v = m[x]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_for_each_over_literal_union_element_array_with_a_non_key() {
        // One member outside the object's keys is enough to widen the lookup.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const u: ('a' | 'zzz')[] = ['a', 'zzz'];\n\
                   u.forEach((x) => { const v = m[x]; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_for_each_over_empty_literal_tuple() {
        // An empty tuple states no element type.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const t: readonly [] = [];\n\
                   t.forEach((x) => { const v = m[x]; });";
        assert_eq!(run(src).len(), 1);
    }

    // --- #7115: a named tuple member labels an element; the label states no
    // --- type of its own.

    #[test]
    fn allows_for_each_over_named_tuple_of_key_elements() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const t: [first: keyof typeof m, ...rest: (keyof typeof m)[]] = ['a', 'b'];\n\
                   t.forEach((x) => { const v = m[x]; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_for_each_over_named_tuple_of_strings() {
        // The label is looked through by the decider *and* by the gate, so a
        // named `string` element vetoes exactly as a bare one does.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   const t: [first: string, second: string] = ['a', 'b'];\n\
                   t.forEach((x) => { const v = m[x]; });";
        assert_eq!(run(src).len(), 1);
    }

    // --- #7787: a string-literal union behind a type alias.

    #[test]
    fn allows_key_param_typed_by_an_alias_of_a_string_literal_union() {
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   type Event = 'approved' | 'rejected';\n\
                   function f(event: Event) { return m[event]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_key_param_typed_by_an_alias_union_with_a_non_key() {
        // The subset guard survives the alias hop.
        let src = "const m = { approved: 'approvedAt', rejected: 'rejectedAt' } as const;\n\
                   type Event = 'approved' | 'deleted';\n\
                   function f(event: Event) { return m[event]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_key_param_typed_by_a_generic_constrained_to_a_literal_union() {
        // The constraint is read by the same decider as the annotation.
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   function f<T extends 'a' | 'b'>(k: T) { return m[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_key_param_typed_by_a_generic_constrained_to_string() {
        let src = "const m = { a: 1, b: 2 } as const;\n\
                   function f<T extends string>(k: T) { return m[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    // --- symbol-resolved object binding (the per-lookup file scan it replaced
    // --- matched by name).

    #[test]
    fn ignores_index_of_a_same_named_object_from_an_unrelated_scope() {
        // The `m` indexed in `g` is the `string`-keyed parameter's namesake in
        // module scope, not the `as const` object declared inside `f`.
        let src = "function f() { const m = { a: 1, b: 2 } as const; return m['a']; }\n\
                   const m = { a: 1 };\n\
                   function g(k: string) { return m[k]; }";
        assert!(run(src).is_empty());
    }
}
