//! ts-bounded-recursive-generic OXC backend — flag recursive conditional/mapped
//! types that lack a depth accumulator parameter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return;
        };

        let name = alias.id.name.as_str();
        if name.is_empty() {
            return;
        }

        // A type with no type parameters cannot be the unbounded recursive
        // conditional generic this rule targets: there is nowhere to add the
        // depth accumulator the remediation asks for. Structural self-reference
        // in a non-generic object alias (like a recursive interface) is fine.
        if alias.type_parameters.as_ref().is_none_or(|tp| tp.params.is_empty()) {
            return;
        }

        // Must be a conditional or mapped type, decided on the AST node so that
        // comment text or method-signature brackets never trigger it.
        if !is_conditional_or_mapped(&alias.type_annotation) {
            return;
        }

        // Must reference itself as a *type*. Walking the AST (not the source
        // text) ensures a property key or an indexed-access string literal that
        // merely shares the alias name does not count as a recursive call.
        if !type_annotation_references_type(&alias.type_annotation, name, false) {
            return;
        }

        // Must lack both a depth parameter and an accumulator parameter.
        if let Some(type_params) = &alias.type_parameters
            && (has_depth_parameter(type_params, ctx.source)
                || has_accumulator_parameter(type_params))
        {
            return;
        }

        // Exempt self-bounding recursion: a recursive call whose argument is a
        // type variable introduced by `infer` in a conditional's `extends`
        // clause. Such a variable is always a sub-part of the matched input
        // (e.g. the `Tail` of a template-literal string, or the element of
        // `Array<infer Elem>`), so it strictly shrinks each step and the
        // recursion terminates without an explicit depth counter.
        if recurses_on_infer_binding(&alias.type_annotation, name) {
            return;
        }

        // Exempt structural-descent recursion: a recursive call whose argument
        // descends two or more levels into the input via a named container
        // property, e.g. `Recurse<T['states'][K]>`. Indexing through a named
        // property and then into one of its keys lands on a strictly-nested
        // child of the finite input schema, so each step shrinks the type and
        // the recursion is bounded by the schema's nesting depth. A single-level
        // index like `T[keyof T]` or `T["next"]` carries no such guarantee and
        // is still flagged.
        if recurses_on_nested_member_descent(&alias.type_annotation, name) {
            return;
        }

        // Exempt union-shrinking recursion: a recursive call whose argument is
        // `Exclude<U, M>`. `Exclude` is TypeScript's union-difference utility, so
        // its result is a subset of `U`; peeling a member off the union each step
        // (the union-to-tuple pattern) drives the input toward `never`, where the
        // conditional base case terminates. The recursion is bounded by the
        // union's cardinality, so no numeric depth counter is needed.
        if recurses_on_excluded_union(&alias.type_annotation, name) {
            return;
        }

        // Exempt mapped-key-index recursion: a recursive call whose argument is
        // `T[K]`, where `K` is the key variable of an enclosing mapped type
        // (`{ [K in keyof T]: Recurse<T[K]> }`). Indexing the input at the key
        // currently being iterated lands on a direct child value, so each step
        // descends one level into the input's finite nesting and the conditional's
        // primitive branch is the base case. A naked `T[keyof T]`, where the index
        // is not a mapped key, carries no such guarantee and is still flagged.
        if recurses_on_mapped_key_index(&alias.type_annotation, name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Recursive type `{name}` has no depth parameter; add one to bound recursion."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Return true if `ty` is a conditional or mapped type, looking through
/// parenthesized wrappers. These are the only type forms that can blow up the
/// TypeScript type-checker through unbounded recursion.
fn is_conditional_or_mapped(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    match ty {
        TSType::TSConditionalType(_) | TSType::TSMappedType(_) => true,
        TSType::TSParenthesizedType(paren) => is_conditional_or_mapped(&paren.type_annotation),
        _ => false,
    }
}

/// Return true if `ty` contains a `TSTypeReference` whose name is `name`,
/// i.e. the alias genuinely references itself as a *type*. Property keys and
/// indexed-access string literals that share the name are not type references,
/// so they do not match. `shadowed` is true once an enclosing conditional's
/// `extends` clause binds an `infer <name>` of the same name: under that binding
/// a *bare* (no-type-argument) reference to `name` is the inferred variable, not
/// a self-call, so it does not count. A reference *with* type arguments
/// (`name<...>`) cannot apply to an inferred type variable, so it is always a
/// genuine recursive reference. Shares the conditional/indexed/union traversal
/// shape of [`collect`].
fn type_annotation_references_type(
    ty: &oxc_ast::ast::TSType,
    name: &str,
    shadowed: bool,
) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};

    match ty {
        TSType::TSTypeReference(tref) => {
            let is_self_name = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == name
            );
            match &tref.type_arguments {
                // Applied form `name<...>`: a self-call regardless of shadowing,
                // and its arguments may themselves reference the alias.
                Some(args) => {
                    is_self_name
                        || args
                            .params
                            .iter()
                            .any(|arg| type_annotation_references_type(arg, name, shadowed))
                }
                // Bare form `name`: a self-reference only when not shadowed by an
                // `infer name` binding.
                None => is_self_name && !shadowed,
            }
        }
        TSType::TSConditionalType(cond) => {
            // `infer name` in the `extends` clause shadows the alias name for the
            // rest of this conditional, so bare references below are the inferred
            // variable rather than self-calls.
            let inner_shadowed = shadowed || extends_clause_binds_infer(&cond.extends_type, name);
            type_annotation_references_type(&cond.check_type, name, inner_shadowed)
                || type_annotation_references_type(&cond.extends_type, name, inner_shadowed)
                || type_annotation_references_type(&cond.true_type, name, inner_shadowed)
                || type_annotation_references_type(&cond.false_type, name, inner_shadowed)
        }
        TSType::TSArrayType(arr) => {
            type_annotation_references_type(&arr.element_type, name, shadowed)
        }
        TSType::TSIndexedAccessType(idx) => {
            type_annotation_references_type(&idx.object_type, name, shadowed)
                || type_annotation_references_type(&idx.index_type, name, shadowed)
        }
        TSType::TSUnionType(u) => {
            u.types.iter().any(|t| type_annotation_references_type(t, name, shadowed))
        }
        TSType::TSIntersectionType(i) => {
            i.types.iter().any(|t| type_annotation_references_type(t, name, shadowed))
        }
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| tuple_element_references_type(el, name, shadowed)),
        TSType::TSNamedTupleMember(member) => {
            tuple_element_references_type(&member.element_type, name, shadowed)
        }
        TSType::TSTypeOperatorType(op) => {
            type_annotation_references_type(&op.type_annotation, name, shadowed)
        }
        TSType::TSParenthesizedType(paren) => {
            type_annotation_references_type(&paren.type_annotation, name, shadowed)
        }
        TSType::TSTemplateLiteralType(tpl) => {
            tpl.types.iter().any(|t| type_annotation_references_type(t, name, shadowed))
        }
        TSType::TSMappedType(mapped) => {
            type_annotation_references_type(&mapped.constraint, name, shadowed)
                || mapped
                    .name_type
                    .as_ref()
                    .is_some_and(|t| type_annotation_references_type(t, name, shadowed))
                || mapped
                    .type_annotation
                    .as_ref()
                    .is_some_and(|t| type_annotation_references_type(t, name, shadowed))
        }
        _ => false,
    }
}

/// Return true if `extends_type` contains an `infer <name>` binding, i.e. the
/// conditional's `extends` clause introduces a type variable that shadows the
/// alias name `name`.
fn extends_clause_binds_infer(extends_type: &oxc_ast::ast::TSType, name: &str) -> bool {
    use oxc_ast::ast::TSType;
    match extends_type {
        TSType::TSInferType(infer) => infer.type_parameter.name.name.as_str() == name,
        TSType::TSTypeReference(tref) => tref.type_arguments.as_ref().is_some_and(|args| {
            args.params.iter().any(|arg| extends_clause_binds_infer(arg, name))
        }),
        TSType::TSArrayType(arr) => extends_clause_binds_infer(&arr.element_type, name),
        TSType::TSIndexedAccessType(idx) => {
            extends_clause_binds_infer(&idx.object_type, name)
                || extends_clause_binds_infer(&idx.index_type, name)
        }
        TSType::TSUnionType(u) => u.types.iter().any(|t| extends_clause_binds_infer(t, name)),
        TSType::TSIntersectionType(i) => {
            i.types.iter().any(|t| extends_clause_binds_infer(t, name))
        }
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| tuple_element_binds_infer(el, name)),
        TSType::TSNamedTupleMember(member) => {
            tuple_element_binds_infer(&member.element_type, name)
        }
        TSType::TSTypeOperatorType(op) => extends_clause_binds_infer(&op.type_annotation, name),
        TSType::TSParenthesizedType(paren) => {
            extends_clause_binds_infer(&paren.type_annotation, name)
        }
        TSType::TSFunctionType(f) => {
            function_signature_binds_infer(&f.params, &f.return_type, name)
        }
        TSType::TSConstructorType(c) => {
            function_signature_binds_infer(&c.params, &c.return_type, name)
        }
        TSType::TSTemplateLiteralType(tpl) => {
            tpl.types.iter().any(|t| extends_clause_binds_infer(t, name))
        }
        TSType::TSTypeLiteral(literal) => {
            use oxc_ast::ast::TSSignature;
            literal.members.iter().any(|member| {
                let TSSignature::TSPropertySignature(prop) = member else {
                    return false;
                };
                prop.type_annotation
                    .as_ref()
                    .is_some_and(|ann| extends_clause_binds_infer(&ann.type_annotation, name))
            })
        }
        _ => false,
    }
}

/// Return true if a function or constructor type signature binds an
/// `infer <name>` in any parameter type or the return type — e.g.
/// `(x: infer A) => infer R` in an `extends` clause.
fn function_signature_binds_infer(
    params: &oxc_ast::ast::FormalParameters,
    return_type: &oxc_ast::ast::TSTypeAnnotation,
    name: &str,
) -> bool {
    let param_binds = params.items.iter().any(|param| {
        param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| extends_clause_binds_infer(&ann.type_annotation, name))
    });
    param_binds || extends_clause_binds_infer(&return_type.type_annotation, name)
}

fn tuple_element_binds_infer(el: &oxc_ast::ast::TSTupleElement, name: &str) -> bool {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            extends_clause_binds_infer(&opt.type_annotation, name)
        }
        TSTupleElement::TSRestType(rest) => {
            extends_clause_binds_infer(&rest.type_annotation, name)
        }
        other => other.as_ts_type().is_some_and(|inner| extends_clause_binds_infer(inner, name)),
    }
}

fn tuple_element_references_type(
    el: &oxc_ast::ast::TSTupleElement,
    name: &str,
    shadowed: bool,
) -> bool {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            type_annotation_references_type(&opt.type_annotation, name, shadowed)
        }
        TSTupleElement::TSRestType(rest) => {
            type_annotation_references_type(&rest.type_annotation, name, shadowed)
        }
        other => other
            .as_ts_type()
            .is_some_and(|inner| type_annotation_references_type(inner, name, shadowed)),
    }
}

fn has_depth_parameter(
    type_params: &oxc_ast::ast::TSTypeParameterDeclaration,
    source: &str,
) -> bool {
    for tp in &type_params.params {
        let text = &source[tp.span.start as usize..tp.span.end as usize];
        if text.contains("Depth") || text.contains("Count") {
            return true;
        }
        if text.contains("extends number") || text.contains("extends 0") {
            return true;
        }
    }
    false
}

/// Check if any type parameter defaults to an empty-collection initializer
/// (`[]`, `never`, `{}`, or `''`). Such a default marks the accumulator
/// pattern, where recursion is bounded by structural shrinkage of the input
/// each step rather than by a numeric depth counter.
fn has_accumulator_parameter(type_params: &oxc_ast::ast::TSTypeParameterDeclaration) -> bool {
    use oxc_ast::ast::{TSLiteral, TSType};
    type_params.params.iter().any(|tp| match &tp.default {
        Some(TSType::TSTupleType(tuple)) => tuple.element_types.is_empty(),
        Some(TSType::TSNeverKeyword(_)) => true,
        Some(TSType::TSTypeLiteral(literal)) => literal.members.is_empty(),
        Some(TSType::TSLiteralType(literal)) => {
            matches!(&literal.literal, TSLiteral::StringLiteral(s) if s.value.is_empty())
        }
        _ => false,
    })
}

/// Return true if `annotation` contains a recursive call to `alias_name` whose
/// type argument is a variable bound by `infer` somewhere in the annotation.
fn recurses_on_infer_binding(annotation: &oxc_ast::ast::TSType, alias_name: &str) -> bool {
    let mut infer_names = Vec::new();
    let mut self_call_args = Vec::new();
    collect(annotation, alias_name, &mut infer_names, &mut self_call_args);
    self_call_args.iter().any(|arg| infer_names.contains(arg))
}

/// Walk every nested type, recording `infer` binding names and the top-level
/// identifier type-arguments of recursive calls to `alias_name`.
fn collect<'a>(
    ty: &'a oxc_ast::ast::TSType<'a>,
    alias_name: &str,
    infer_names: &mut Vec<&'a str>,
    self_call_args: &mut Vec<&'a str>,
) {
    use oxc_ast::ast::{TSType, TSTypeName};

    match ty {
        TSType::TSInferType(infer) => {
            infer_names.push(infer.type_parameter.name.name.as_str());
        }
        TSType::TSConditionalType(cond) => {
            collect(&cond.check_type, alias_name, infer_names, self_call_args);
            collect(&cond.extends_type, alias_name, infer_names, self_call_args);
            collect(&cond.true_type, alias_name, infer_names, self_call_args);
            collect(&cond.false_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSTypeReference(tref) => {
            let is_self_call = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == alias_name
            );
            if let Some(args) = &tref.type_arguments {
                for arg in &args.params {
                    if is_self_call
                        && let TSType::TSTypeReference(arg_ref) = arg
                        && let TSTypeName::IdentifierReference(id) = &arg_ref.type_name
                        && arg_ref.type_arguments.is_none()
                    {
                        self_call_args.push(id.name.as_str());
                    }
                    collect(arg, alias_name, infer_names, self_call_args);
                }
            }
        }
        TSType::TSArrayType(arr) => {
            collect(&arr.element_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSIndexedAccessType(idx) => {
            collect(&idx.object_type, alias_name, infer_names, self_call_args);
            collect(&idx.index_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSUnionType(u) => {
            for t in &u.types {
                collect(t, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSIntersectionType(i) => {
            for t in &i.types {
                collect(t, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSTupleType(tuple) => {
            for el in &tuple.element_types {
                collect_tuple_element(el, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSNamedTupleMember(member) => {
            collect_tuple_element(&member.element_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSTypeOperatorType(op) => {
            collect(&op.type_annotation, alias_name, infer_names, self_call_args);
        }
        TSType::TSParenthesizedType(paren) => {
            collect(&paren.type_annotation, alias_name, infer_names, self_call_args);
        }
        TSType::TSFunctionType(f) => {
            collect_function_signature(
                &f.params,
                &f.return_type,
                alias_name,
                infer_names,
                self_call_args,
            );
        }
        TSType::TSConstructorType(c) => {
            collect_function_signature(
                &c.params,
                &c.return_type,
                alias_name,
                infer_names,
                self_call_args,
            );
        }
        TSType::TSTemplateLiteralType(tpl) => {
            for t in &tpl.types {
                collect(t, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSMappedType(mapped) => {
            collect(&mapped.constraint, alias_name, infer_names, self_call_args);
            if let Some(name_type) = &mapped.name_type {
                collect(name_type, alias_name, infer_names, self_call_args);
            }
            if let Some(annotation) = &mapped.type_annotation {
                collect(annotation, alias_name, infer_names, self_call_args);
            }
        }
        _ => {}
    }
}

/// Walk a function or constructor type signature, recording `infer` bindings in
/// its parameter and return types — e.g. the `infer R` of
/// `(...args: any[]) => infer R` in an `extends` clause. The function's return
/// type is a sub-part of the matched input, so a recursive call fed that `infer`
/// binding (the function-return-unwinding pattern) strictly shrinks each step.
fn collect_function_signature<'a>(
    params: &'a oxc_ast::ast::FormalParameters<'a>,
    return_type: &'a oxc_ast::ast::TSTypeAnnotation<'a>,
    alias_name: &str,
    infer_names: &mut Vec<&'a str>,
    self_call_args: &mut Vec<&'a str>,
) {
    for param in &params.items {
        if let Some(ann) = &param.type_annotation {
            collect(&ann.type_annotation, alias_name, infer_names, self_call_args);
        }
    }
    collect(&return_type.type_annotation, alias_name, infer_names, self_call_args);
}

fn collect_tuple_element<'a>(
    el: &'a oxc_ast::ast::TSTupleElement<'a>,
    alias_name: &str,
    infer_names: &mut Vec<&'a str>,
    self_call_args: &mut Vec<&'a str>,
) {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            collect(&opt.type_annotation, alias_name, infer_names, self_call_args);
        }
        TSTupleElement::TSRestType(rest) => {
            collect(&rest.type_annotation, alias_name, infer_names, self_call_args);
        }
        other => {
            if let Some(inner) = other.as_ts_type() {
                collect(inner, alias_name, infer_names, self_call_args);
            }
        }
    }
}

/// Return true if `annotation` contains a recursive call to `alias_name` whose
/// type argument descends two or more levels into a base type via a named
/// container property — e.g. `Recurse<T['states'][K]>`. Such an argument is a
/// strictly-nested child of the finite input schema, so the recursion is
/// bounded by the schema's nesting depth.
fn recurses_on_nested_member_descent(
    annotation: &oxc_ast::ast::TSType,
    alias_name: &str,
) -> bool {
    let mut found = false;
    visit_self_call_args(annotation, alias_name, &[], &mut |arg, _mapped_keys| {
        if is_nested_member_descent(arg) {
            found = true;
        }
    });
    found
}

/// Walk every nested type and invoke `on_arg` for each top-level type-argument
/// of a recursive call to `alias_name`. `mapped_keys` carries the key variables
/// of the mapped types enclosing the current position (`K` in `[K in keyof T]`),
/// so a callback can recognize a `T[K]` descent into the input.
fn visit_self_call_args<'a>(
    ty: &'a oxc_ast::ast::TSType<'a>,
    alias_name: &str,
    mapped_keys: &[&'a str],
    on_arg: &mut impl FnMut(&'a oxc_ast::ast::TSType<'a>, &[&'a str]),
) {
    use oxc_ast::ast::{TSType, TSTypeName};

    match ty {
        TSType::TSConditionalType(cond) => {
            visit_self_call_args(&cond.check_type, alias_name, mapped_keys, on_arg);
            visit_self_call_args(&cond.extends_type, alias_name, mapped_keys, on_arg);
            visit_self_call_args(&cond.true_type, alias_name, mapped_keys, on_arg);
            visit_self_call_args(&cond.false_type, alias_name, mapped_keys, on_arg);
        }
        TSType::TSTypeReference(tref) => {
            let is_self_call = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == alias_name
            );
            if let Some(args) = &tref.type_arguments {
                for arg in &args.params {
                    if is_self_call {
                        on_arg(arg, mapped_keys);
                    }
                    visit_self_call_args(arg, alias_name, mapped_keys, on_arg);
                }
            }
        }
        TSType::TSArrayType(arr) => {
            visit_self_call_args(&arr.element_type, alias_name, mapped_keys, on_arg);
        }
        TSType::TSIndexedAccessType(idx) => {
            visit_self_call_args(&idx.object_type, alias_name, mapped_keys, on_arg);
            visit_self_call_args(&idx.index_type, alias_name, mapped_keys, on_arg);
        }
        TSType::TSUnionType(u) => {
            for t in &u.types {
                visit_self_call_args(t, alias_name, mapped_keys, on_arg);
            }
        }
        TSType::TSIntersectionType(i) => {
            for t in &i.types {
                visit_self_call_args(t, alias_name, mapped_keys, on_arg);
            }
        }
        TSType::TSTupleType(tuple) => {
            for el in &tuple.element_types {
                if let Some(inner) = tuple_element_as_type(el) {
                    visit_self_call_args(inner, alias_name, mapped_keys, on_arg);
                }
            }
        }
        TSType::TSNamedTupleMember(member) => {
            if let Some(inner) = tuple_element_as_type(&member.element_type) {
                visit_self_call_args(inner, alias_name, mapped_keys, on_arg);
            }
        }
        TSType::TSTypeOperatorType(op) => {
            visit_self_call_args(&op.type_annotation, alias_name, mapped_keys, on_arg);
        }
        TSType::TSParenthesizedType(paren) => {
            visit_self_call_args(&paren.type_annotation, alias_name, mapped_keys, on_arg);
        }
        TSType::TSTemplateLiteralType(tpl) => {
            for t in &tpl.types {
                visit_self_call_args(t, alias_name, mapped_keys, on_arg);
            }
        }
        TSType::TSMappedType(mapped) => {
            // The key `K` is bound by `in`, so it is out of scope in the
            // constraint (`keyof T`) but in scope for the name and value types.
            visit_self_call_args(&mapped.constraint, alias_name, mapped_keys, on_arg);
            let mut inner_keys = mapped_keys.to_vec();
            inner_keys.push(mapped.key.name.as_str());
            if let Some(name_type) = &mapped.name_type {
                visit_self_call_args(name_type, alias_name, &inner_keys, on_arg);
            }
            if let Some(annotation) = &mapped.type_annotation {
                visit_self_call_args(annotation, alias_name, &inner_keys, on_arg);
            }
        }
        _ => {}
    }
}

fn tuple_element_as_type<'a>(
    el: &'a oxc_ast::ast::TSTupleElement<'a>,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => Some(&opt.type_annotation),
        TSTupleElement::TSRestType(rest) => Some(&rest.type_annotation),
        other => other.as_ts_type(),
    }
}

/// Return true if `annotation` contains a recursive call to `alias_name` whose
/// type argument is `Exclude<U, M>`. `Exclude` removes members from a union, so
/// the argument is a subset of `U`; recursing on it shrinks the input toward
/// `never` and the recursion is bounded by the union's cardinality.
fn recurses_on_excluded_union(annotation: &oxc_ast::ast::TSType, alias_name: &str) -> bool {
    let mut found = false;
    visit_self_call_args(annotation, alias_name, &[], &mut |arg, _mapped_keys| {
        if is_exclude_application(arg) {
            found = true;
        }
    });
    found
}

/// Return true if `annotation` contains a recursive call to `alias_name` whose
/// type argument is `T[K]`, where `T` is a bare type reference and `K` is the
/// key variable of an enclosing mapped type (`{ [K in keyof T]: ... }`). Each
/// such step indexes the input at the key currently being iterated, landing on a
/// strictly-nested child value, so the recursion is bounded by the input's
/// nesting depth and terminates at the conditional's primitive base case.
fn recurses_on_mapped_key_index(annotation: &oxc_ast::ast::TSType, alias_name: &str) -> bool {
    let mut found = false;
    visit_self_call_args(annotation, alias_name, &[], &mut |arg, mapped_keys| {
        if is_mapped_key_index(arg, mapped_keys) {
            found = true;
        }
    });
    found
}

/// Return true if `arg` is `T[K]`: an indexed access whose object is a bare type
/// reference and whose index is a bare type reference naming one of
/// `mapped_keys` — the key variables of the enclosing mapped types. `T[K]` reads
/// the input at the key currently being iterated, a direct child value, so the
/// recursion descends one level each step. A naked `T[keyof T]` (the index is
/// `keyof T`, not a mapped key) or a free index variable is not matched.
fn is_mapped_key_index(arg: &oxc_ast::ast::TSType, mapped_keys: &[&str]) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSIndexedAccessType(idx) = arg else {
        return false;
    };
    if !is_bare_type_reference(&idx.object_type) {
        return false;
    }
    let TSType::TSTypeReference(index_ref) = &idx.index_type else {
        return false;
    };
    let TSTypeName::IdentifierReference(id) = &index_ref.type_name else {
        return false;
    };
    index_ref.type_arguments.is_none() && mapped_keys.contains(&id.name.as_str())
}

/// Return true if `arg` is an application of the built-in `Exclude` utility type
/// with at least the two type arguments it requires (`Exclude<U, M>`).
fn is_exclude_application(arg: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = arg else {
        return false;
    };
    let is_exclude = matches!(
        &tref.type_name,
        TSTypeName::IdentifierReference(id) if id.name.as_str() == "Exclude"
    );
    is_exclude
        && tref
            .type_arguments
            .as_ref()
            .is_some_and(|args| args.params.len() >= 2)
}

/// Return true if `arg` is an indexed access that descends at least two levels
/// into a base type via a named (string-literal) container property, where the
/// innermost base is a bare type reference — e.g. `T['states'][K]`. The named
/// container property is what guarantees descent into a distinct nested child:
/// a single-level access (`T[keyof T]`, `T["next"]`) does not.
fn is_nested_member_descent(arg: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    let TSType::TSIndexedAccessType(outer) = arg else {
        return false;
    };
    // The outer object must itself be an indexed access whose index is a named
    // (string-literal) container property, rooted at a bare type reference.
    let TSType::TSIndexedAccessType(inner) = &outer.object_type else {
        return false;
    };
    is_string_literal_index(&inner.index_type) && is_bare_type_reference(&inner.object_type)
}

/// Return true if `ty` is a string-literal type (the index of a named property
/// access like `T['states']`).
fn is_string_literal_index(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSLiteral, TSType};
    matches!(
        ty,
        TSType::TSLiteralType(lit) if matches!(&lit.literal, TSLiteral::StringLiteral(_))
    )
}

/// Return true if `ty` is a bare type reference with no type arguments
/// (the input type variable rooting the descent).
fn is_bare_type_reference(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    matches!(ty, TSType::TSTypeReference(tref) if tref.type_arguments.is_none())
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
    fn exempts_union_to_tuple_accumulator() {
        let src = r#"
export type UnionToTuple<
  union,
  output extends any[] = []
> = UnionToIntersection<union extends any ? (t: union) => union : never> extends (_: any) => infer elem
  ? UnionToTuple<Exclude<union, elem>, [elem, ...output]>
  : output;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_flatten_accumulator() {
        let src = r#"
export type Flatten<
  xs extends readonly any[],
  output extends any[] = []
> = xs extends readonly [infer head, ...infer tail]
  ? Flatten<tail, [...output, ...Extract<head, readonly any[]>]>
  : output;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_minimal_tuple_accumulator() {
        let src = "type Acc<T, Out extends any[] = []> = T extends [infer H, ...infer R] ? Acc<R, [...Out, H]> : Out;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_never_accumulator() {
        let src = "type Values<T, Out = never> = T extends [infer H, ...infer R] ? Values<R, Out | H> : Out;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_depth_parameter() {
        let src = "type Walk<T, Depth extends number = 0> = T extends object ? Walk<T[keyof T], Depth> : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_template_literal_string_shrink() {
        let src = r#"
export type PathParameters<
  TRoute extends string,
> = TRoute extends `${infer _Head}/{${infer _Param}}${infer Tail}`
  ? [pathParameter: string, ...pathParameters: PathParameters<Tail>]
  : [];
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_infer_from_array_element() {
        let src =
            "type Deep<T> = T extends Array<infer Elem> ? Deep<Elem> : T extends object ? string : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unbounded_single_param_recursion() {
        let src = "type InfiniteLoop<T> = T extends any ? InfiniteLoop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_recursion_on_original_input_not_infer() {
        let src = "type Loop<T> = T extends `${infer _Head}${infer _Tail}` ? Loop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_non_accumulator_default() {
        let src = "type Loop<T, Fallback = string> = T extends any ? Loop<T, Fallback> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_non_generic_self_referential_alias() {
        let src = r#"
export type KyInstance = {
    get: <T>(url: string) => T;
    /** Will be undefined in case ky.stop is returned, see options[0]. */
    create: (defaultOptions?: Options) => KyInstance;
    extend: (defaultOptions: Options) => KyInstance;
};
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_non_generic_recursive_object_alias() {
        let src = "type Node = { children: Node[] };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unbounded_recursive_conditional_generic() {
        let src = "type Deep<T> = T extends object ? Deep<T[keyof T]> : T;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_property_key_name_collision() {
        let src = "export type value<T> = T extends { value: infer V } ? V : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_indexed_access_string_literal_name_collision() {
        let src =
            r#"export type input<T> = T extends { _zod: { input: any } } ? T["_zod"]["input"] : unknown;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_self_reference_with_string_literal_index_in_body() {
        let src = "type Deep<T> = T extends { next: infer N } ? Deep<T[\"next\"]> : T;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_nested_member_descent_xstate_state_value() {
        let src = r#"
export type ToStateValue<T extends StateSchema> = T extends {
  states: Record<infer S, any>;
}
  ? IsNever<S> extends true
    ? {}
    : { [K in keyof T["states"]]: ToStateValue<T["states"][K]> }
  : {};
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_nested_member_descent_items_container() {
        let src = "type Walk<T> = T extends { items: object } ? Walk<T[\"items\"][number]> : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_tuplify_union_termination_flag_default() {
        // TuplifyUnion recurses on `Exclude<T, L>`, peeling one member off the
        // union each step; termination is the `[T] extends [never]` base case.
        let src = r#"
export type TuplifyUnion<
  T,
  L = LastOf<T>,
  N = [T] extends [never] ? true : false
> = true extends N ? [] : Push<TuplifyUnion<Exclude<T, L>>, L>;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_union_shrink_without_accumulator() {
        let src =
            "type U2T<T> = [T] extends [never] ? [] : [Last<T>, ...U2T<Exclude<T, Last<T>>>];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_exclude_on_growing_self_call_arg() {
        // The self-call argument is `T`, not an `Exclude<...>`, so the input does
        // not shrink and the recursion stays unbounded even though `Exclude`
        // appears elsewhere in the body.
        let src = "type Loop<T> = T extends Exclude<any, never> ? Loop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_exclude_with_single_arg_not_exempted() {
        // A degenerate `Exclude<T>` (one argument) is not a union difference and
        // proves no shrinkage, so the recursion stays flagged.
        let src = "type Loop<T> = [T] extends [never] ? [] : Loop<Exclude<T>>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_infer_binding_returned_directly_tuple_rest() {
        // `Tail` in the true branch is the `infer Tail` tuple-rest binding, not a
        // recursive call to the alias `Tail`. The alias is not recursive at all.
        let src = r#"
export type Tail<T extends any[]> = T extends [any, ...infer Tail]
  ? Tail
  : never;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_infer_binding_returned_directly_object_property() {
        // `GetSerializedErrorType` in the true branch is the `infer
        // GetSerializedErrorType` binding from the object pattern, not a self-call.
        let src = r#"
type GetSerializedErrorType<ThunkApiConfig> = ThunkApiConfig extends {
  serializedErrorType: infer GetSerializedErrorType;
}
  ? GetSerializedErrorType
  : SerializedError;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_infer_binding_in_function_parameter() {
        // `F` in the true branch is the `infer F` parameter binding, not a self-call.
        let src = "type F<T> = T extends (x: infer F) => any ? F : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_infer_binding_in_function_return() {
        // `R` in the true branch is the `infer R` return-type binding, not a self-call.
        let src = "type R<T> = T extends () => infer R ? R : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_applied_self_call_despite_infer_binding_same_name() {
        // Even when an `infer Loop` shadows the name, an *applied* `Loop<...>` is a
        // genuine recursive call (type arguments cannot apply to an inferred
        // variable). The argument `T` does not shrink, so the unbounded recursion
        // stays flagged.
        let src = "type Loop<T> = T extends { x: infer Loop } ? Loop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_single_level_named_property_index_still_unbounded() {
        // A single named-property access (`T["items"]`) is only one level of
        // descent, not a member-descent into a nested child, so it stays flagged
        // — unlike the two-level `T["items"][K]`.
        let src = "type Walk<T> = T extends { items: object } ? Walk<T[\"items\"]> : T;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_function_return_unwinding_shape_from_id_columns() {
        // ShapeFromIdColumns recurses on `R`, the `infer R` return-type binding of
        // the factory-function branch. Each step unwraps one function layer, so the
        // recursion is bounded by the function-nesting depth and the `never` branch
        // is the base case.
        let src = r#"
export type ShapeFromIdColumns<
  Types extends SchemaTypes,
  Table extends keyof Types['DrizzleRelations'],
  IDColumns,
> = IDColumns extends Column
  ? IDColumns['_']['data']
  : IDColumns extends Column[]
    ? {
        [K in IDColumns[number]['_']['name']]: Extract<
          IDColumns[number],
          { _: { name: K } }
        >['_']['data'];
      }
    : IDColumns extends ((...args: any[]) => infer R extends Column | Column[])
      ? ShapeFromIdColumns<Types, Table, R>
      : never;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_function_return_unwinding_minimal() {
        // Minimal function-return-unwinding: recurse on the `infer R` return-type
        // binding of a function-type `extends` clause.
        let src = "type Unwrap<T> = T extends () => infer R ? Unwrap<R> : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_constructor_return_unwinding() {
        // Same pattern via a constructor type signature.
        let src = "type Unwrap<T> = T extends new () => infer R ? Unwrap<R> : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_function_type_self_call_on_original_input() {
        // The function-type branch binds `infer R`, but the recursive call passes
        // the original input `T`, not `R`. The input does not shrink, so the
        // recursion stays unbounded and must still flag.
        let src = "type Loop<T> = T extends () => infer R ? Loop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_mapped_key_index_deep_readonly() {
        // DeepReadonly recurses on `TValue[TKey]`, where `TKey` is the enclosing
        // mapped type's key variable, descending one level into the input each
        // step until the conditional's primitive branch terminates it.
        let src = r#"
export type DeepReadonly<TValue> = TValue extends
  | Record<string, unknown>
  | readonly unknown[]
  ? { readonly [TKey in keyof TValue]: DeepReadonly<TValue[TKey]> }
  : TValue;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_mapped_key_index_infer_fallbacks() {
        // Minimal mapped-key-index recursion: `InferFallbacks<TEntries[TKey]>`
        // inside a `-readonly [TKey in keyof TEntries]` mapped type body.
        let src = r#"
type InferFallbacks<TSchema> = TSchema extends Schema<infer TEntries>
  ? { -readonly [TKey in keyof TEntries]: InferFallbacks<TEntries[TKey]> }
  : never;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_free_index_variable_not_mapped_key() {
        // The self-call argument `T[K]` uses `K`, a type parameter of the alias
        // rather than the key variable of any enclosing mapped type, so it carries
        // no descent guarantee and the recursion stays unbounded.
        let src = "type Deep<T, K extends keyof T> = T extends object ? Deep<T[K], K> : T;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_index_by_free_variable_inside_mapped_type() {
        // A mapped type encloses the recursive call, but the index `K` is a free
        // alias parameter, not the mapped type's key variable `P`. Indexing by an
        // unrelated variable does not descend per-key, so the recursion stays
        // unbounded even though a mapped key (`P`) is in scope.
        let src = "type Loop<T, K extends keyof T> = T extends object ? { [P in keyof T]: Loop<T[K], K> } : T;";
        assert_eq!(run_on(src).len(), 1);
    }
}
