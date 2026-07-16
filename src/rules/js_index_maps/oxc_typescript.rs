//! OxcCheck backend for js-index-maps — flag a bare-identifier
//! `.find()`/`.findIndex()`/`.filter()`/`.includes()`/`.indexOf()` inside a loop
//! as a possible O(n*m) array scan. EXCEPTIONS: `.includes()`/`.indexOf()` whose
//! sole argument is a string literal, or whose receiver is statically a string
//! (a string literal/template, a string-returning call like `s.toLowerCase()`,
//! an identifier bound to a `const` initialized to one of those
//! (`const lc = s.toLowerCase(); lc.includes(x)`), or an identifier narrowed to
//! `string` by a `typeof x === "string"` guard in an enclosing `&&` chain), is a
//! `String.prototype` substring search, not array membership — there is no
//! collection to index, so it is not flagged;
//! a two-argument `.indexOf(value, fromIndex)` is a forward-scan cursor (a
//! positional string/array walk), never a membership lookup, so it is not flagged;
//! a method-call chain rooted in an inline literal array (`["./", "/"].includes(x)`,
//! `[a, b].flat().filter(Boolean)`) has a fixed, hardcoded size independent of
//! input, so the scan is O(1), not flagged;
//! an identifier bound to a `const` NON-EMPTY inline array literal
//! (`const valid = ["yes", "no"]; valid.includes(x)`) is the same fixed-size
//! lookup table one binding removed, so the scan is O(1), not flagged (an
//! empty-array init like `const seen = []` is a growing accumulator and IS
//! still flagged);
//! a lookup in the iterable expression of a `for..of`/`for..in`
//! (`for (const x of arr.filter(...))`) runs once before the loop, not per
//! iteration, so it is not an O(n*m) site for that loop (an enclosing outer loop
//! is still detected).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, peel_parens};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BinaryExpression, BinaryOperator, CallExpression, ChainElement, Expression,
    LogicalOperator, NewExpression, Statement, TSSignature, TSType, TSTypeAnnotation, TSTypeName,
    TSTypeReference, UnaryOperator, VariableDeclarationKind,
};
use oxc_semantic::ReferenceFlags;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const LOOKUP_METHODS: &[&str] = &["find", "findIndex", "filter", "includes", "indexOf"];
/// Methods whose callback is invoked once per element of the receiver — a
/// per-iteration context. Covers the iterator methods (`forEach`/`map`/…) plus
/// the predicate-taking lookups (`filter`/`find`/`findIndex`): a lookup nested in
/// such a callback runs per element.
const CALLBACK_ITERATING_METHODS: &[&str] =
    &["forEach", "map", "flatMap", "reduce", "some", "every", "filter", "find", "findIndex"];
/// Methods whose presence in a `filter`/`find`/`findIndex` callback body marks a
/// membership scan of a collection — the O(n*m) work a `Map`/`Set` could replace.
const MEMBERSHIP_METHODS: &[&str] = &["includes", "indexOf", "find", "has"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Match `.find(...)`, `.findIndex(...)`, etc.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !LOOKUP_METHODS.contains(&method) {
            return;
        }

        // `String.prototype.includes`/`indexOf` take a search STRING. A sole
        // string-literal argument (`x.includes("figma")`, `x.indexOf("/")`) is a
        // substring search — there is no array to index into a Map/Set, so the
        // O(1)-lookup advice is a category error. Array-membership checks pass a
        // variable/element, not a literal substring. (`find`/`findIndex`/`filter`
        // take a callback, so this only affects `includes`/`indexOf`.)
        if matches!(method, "includes" | "indexOf")
            && call.arguments.len() == 1
            && matches!(call.arguments.first(), Some(Argument::StringLiteral(_)))
        {
            return;
        }

        // A two-argument `indexOf(value, fromIndex)` is a forward-scan cursor: it
        // finds the next occurrence starting at an offset, walking a string/array
        // positionally (`s.indexOf('}', i + 3)`). There is no membership
        // collection to replace with a Map/Set, regardless of receiver type —
        // `Array`/`String.prototype.indexOf` both take exactly
        // `(searchValue, fromIndex)`.
        if method == "indexOf" && call.arguments.len() == 2 {
            return;
        }

        // `String.prototype.includes`/`indexOf` is a substring search, not array
        // membership — `"abc".includes(x)` or `s.toLowerCase().includes(query)`
        // has no collection to hash into a Map/Set, so the O(1)-lookup advice is a
        // category error. Skip when the receiver is statically a string: a string
        // literal/template, a call to a string-returning method, an identifier
        // bound to a `const` initialized to one of those
        // (`const lc = s.toLowerCase(); lc.includes(x)`), or an identifier narrowed
        // to `string` by a `typeof x === "string"` guard in an enclosing `&&` chain
        // (`typeof preset === "string" && preset.includes(word)`).
        if matches!(method, "includes" | "indexOf")
            && (receiver_is_string(&member.object)
                || receiver_is_const_bound_string(&member.object, semantic)
                || receiver_is_typeof_narrowed_string(&member.object, node, semantic))
        {
            return;
        }

        // Skip when the receiver is itself a property access (e.g. product.correspondences.find(...))
        // — relation fields are typically small and bounded; Map materialisation would be worse.
        if matches!(
            &member.object,
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
        ) {
            return;
        }

        // Skip when the method-call chain is rooted in an inline literal array
        // (`["./", "/"].includes(x)`, `[a, b].flat().filter(Boolean)`). The array
        // is spelled out at the call site rather than read from unbounded input,
        // so it is not the growing collection scanned per iteration that the rule
        // targets; building a Set/Map from it would only add allocation overhead
        // with no asymptotic gain.
        if root_receiver_is_literal_array(&member.object) {
            return;
        }

        // Skip when the receiver is an identifier bound to a `const` non-empty
        // inline array literal (`const valid = ["yes", "no"]; valid.includes(x)`).
        // The binding is immutable and the array's size is fixed at the
        // declaration site, so the scan is O(constant) — structurally the inline
        // `["yes", "no"].includes(x)` form one `const` binding removed.
        if receiver_is_const_bound_nonempty_array(&member.object, semantic) {
            return;
        }

        if !is_inside_loop(node, semantic) {
            return;
        }

        // The lookup is already O(1) when the callback predicate is a
        // `.has()` on a known `Set`/`Map` — the index the rule would suggest
        // building already exists.
        if callback_is_known_set_lookup(call, semantic) {
            return;
        }

        // `filter`/`find`/`findIndex` are O(n*m) only when their callback
        // actually scans a collection captured from the enclosing scope — the
        // work a `Map`/`Set` could replace. A bare named predicate, a
        // literal-only/side-effecting callback, or a plain property-truthiness
        // callback does no such scan. (`includes`/`indexOf` take a value, not a
        // callback, and are inherent membership lookups, so they skip this.)
        if matches!(method, "find" | "findIndex" | "filter")
            && !callback_does_captured_lookup(call, semantic)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}()` inside a loop is O(n*m) — build a `Map` or `Set` for O(1) lookups."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the root receiver of a method-call chain is an inline array literal.
/// Walks `CallExpression` chains down through each call's member-expression callee
/// (`[a, b].flat().filter(...)` → `[a, b].flat()` → `[a, b]`) until it reaches the
/// ultimate receiver, returning true iff that root is an `Expression::ArrayExpression`.
/// The base case is the direct `[...].filter(...)` receiver; intermediate method
/// calls (`.flat()`, `.slice()`, …) are walked through to reach the root.
fn root_receiver_is_literal_array(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::CallExpression(call) => match &call.callee {
            Expression::StaticMemberExpression(m) => root_receiver_is_literal_array(&m.object),
            Expression::ComputedMemberExpression(m) => root_receiver_is_literal_array(&m.object),
            _ => false,
        },
        _ => false,
    }
}

/// True when `expr` is an identifier bound to a `const` declaration whose
/// initializer is a NON-EMPTY inline array literal. Such a binding names a
/// fixed-size lookup table at the declaration site — structurally the inline
/// `["yes", "no"].includes(x)` form one binding removed, so a membership scan
/// over it is O(constant) and building a Set/Map would only add allocation
/// overhead. An empty-array init (`const seen = []`) is excluded: it is a
/// growing accumulator (`seen.push(x)`), the genuine O(n*m) collection the rule
/// targets. `let`/`var` bindings are excluded too: they could be reassigned to a
/// larger array, so the size is not statically bounded.
fn receiver_is_const_bound_nonempty_array<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(id) = expr else {
        return false;
    };
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const {
        return false;
    }
    matches!(&decl.init, Some(Expression::ArrayExpression(arr)) if !arr.elements.is_empty())
}

/// Peel a possibly parenthesized / optional-chained predicate down to the
/// innermost `CallExpression`. An optional-chained membership call
/// (`set?.has(x)`) parses as a `ChainExpression` wrapping the `CallExpression`,
/// and `(set.has(x))` wraps it in parentheses — both are the same O(1)
/// membership call as the bare `set.has(x)` form.
fn unwrap_to_call<'a>(expr: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
    match peel_parens(expr) {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// True when `call`'s callback predicate is a (possibly negated) `.has()`
/// lookup whose receiver is structurally known to be a `Set` or `Map`. Such a
/// lookup is O(1), so the flagged method does no O(n*m) scan.
fn callback_is_known_set_lookup<'a>(
    call: &CallExpression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(Argument::ArrowFunctionExpression(arrow)) = call.arguments.first() else {
        return false;
    };
    if !arrow.expression {
        return false;
    }
    let Some(Statement::ExpressionStatement(stmt)) = arrow.body.statements.first() else {
        return false;
    };

    let mut predicate = &stmt.expression;
    while let Expression::UnaryExpression(unary) = predicate {
        if unary.operator != UnaryOperator::LogicalNot {
            return false;
        }
        predicate = &unary.argument;
    }

    let Some(lookup) = unwrap_to_call(predicate) else {
        return false;
    };
    let Expression::StaticMemberExpression(lookup_member) = &lookup.callee else {
        return false;
    };
    if lookup_member.property.name.as_str() != "has" {
        return false;
    }
    is_known_set_or_map(&lookup_member.object, semantic)
}

/// True for a `filter`/`find`/`findIndex` whose callback body performs a
/// membership/equality lookup against a value captured from the enclosing
/// scope — the O(n*m) work a `Map`/`Set` could replace. The signal is either a
/// nested membership call (`.includes()`/`.indexOf()`/`.find()`/`.has()`), or an
/// `===`/`==`/`in` comparison one of whose operands resolves to a free variable
/// (a binding declared OUTSIDE the callback, i.e. not one of its parameters or
/// locals). A bare named predicate (`arr.filter(isValid)`), a literal-only or
/// side-effecting callback (`(m) => m === 'x' ? fx() : true`), and a plain
/// property-truthiness callback (`(x) => x.active`) do no such scan.
fn callback_does_captured_lookup<'a>(
    call: &CallExpression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let (callback_span, body_span) = match call.arguments.first() {
        Some(Argument::ArrowFunctionExpression(arrow)) => (arrow.span, arrow.body.span()),
        Some(Argument::FunctionExpression(func)) => {
            let Some(body) = &func.body else {
                return false;
            };
            (func.span, body.span())
        }
        // A bare identifier / member reference (`arr.filter(isValid)`) is an
        // opaque predicate — no visible lookup to key on.
        _ => return false,
    };

    semantic.nodes().iter().any(|descendant| {
        if !body_span.contains_inclusive(descendant.kind().span()) {
            return false;
        }
        match descendant.kind() {
            AstKind::CallExpression(inner) => matches!(
                &inner.callee,
                Expression::StaticMemberExpression(m)
                    if MEMBERSHIP_METHODS.contains(&m.property.name.as_str())
            ),
            AstKind::BinaryExpression(bin)
                if matches!(
                    bin.operator,
                    BinaryOperator::Equality | BinaryOperator::StrictEquality | BinaryOperator::In
                ) =>
            {
                operand_is_free_variable(&bin.left, callback_span, semantic)
                    || operand_is_free_variable(&bin.right, callback_span, semantic)
            }
            _ => false,
        }
    })
}

/// True when `expr`'s root identifier resolves to a binding declared OUTSIDE
/// `callback_span` — a value captured from the enclosing scope (or an
/// unresolved/global name). The root of a member chain is its head object
/// (`item` for `item.id`, `items` for `items[i].id`). Literals and other
/// non-identifier-rooted operands are never free variables.
fn operand_is_free_variable<'a>(
    expr: &Expression<'a>,
    callback_span: oxc_span::Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cursor = expr;
    let id = loop {
        match cursor {
            Expression::Identifier(id) => break id,
            Expression::StaticMemberExpression(m) => cursor = &m.object,
            Expression::ComputedMemberExpression(m) => cursor = &m.object,
            _ => return false,
        }
    };
    let Some(ref_id) = id.reference_id.get() else {
        return true;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        // Unresolved reference: not a callback-local binding — a free name the
        // callback compares against (`x.id === key` where `key` is captured).
        return true;
    };
    let decl_span = semantic
        .nodes()
        .kind(scoping.symbol_declaration(sym_id))
        .span();
    !callback_span.contains_inclusive(decl_span)
}

/// True when `expr` is structurally a `Set`/`Map`: a direct `new Set(...)` /
/// `new Map(...)`, or an identifier — never reassigned — that is either
/// initialized to one of those or carries a `Set<…>`/`Map<…>` type annotation.
/// The annotation may sit on a variable declaration (`const s: Set<string> =
/// …`), directly on a parameter (`(s: Set<string>) => …`), or on a member
/// destructured from a typed params object (`{ excludedColumns }:
/// WriteRowsOptions` where `excludedColumns: Set<string>`).
fn is_known_set_or_map<'a>(expr: &Expression<'a>, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    match expr {
        Expression::NewExpression(new_expr) => is_set_or_map_constructor(new_expr),
        Expression::Identifier(id) => {
            let Some(ref_id) = id.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            // A reassigned binding loses its declared-type / initializer guarantee.
            if scoping
                .get_resolved_references(sym_id)
                .any(|reference| reference.flags().contains(ReferenceFlags::Write))
            {
                return false;
            }
            let nodes = semantic.nodes();
            let decl_node_id = scoping.symbol_declaration(sym_id);
            // `const m = new Set(…)` / `new Map(…)` initializer, or a `Set<…>` /
            // `Map<…>` variable annotation (`const s: Set<string> = …`).
            if let AstKind::VariableDeclarator(decl) = nodes.kind(decl_node_id) {
                let init_is_constructor = matches!(
                    &decl.init,
                    Some(Expression::NewExpression(n)) if is_set_or_map_constructor(n)
                );
                let annotation_is_set_or_map = decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_is_set_or_map(&ann.type_annotation));
                return init_is_constructor || annotation_is_set_or_map;
            }
            // A parameter (or a member destructured from a typed params object)
            // whose declared type resolves to `Set<…>` / `Map<…>`.
            let binding_name = scoping.symbol_name(sym_id);
            std::iter::once(nodes.kind(decl_node_id))
                .chain(nodes.ancestor_kinds(decl_node_id))
                .any(|kind| match kind {
                    AstKind::FormalParameter(param) => param
                        .type_annotation
                        .as_ref()
                        .is_some_and(|ann| {
                            param_binding_is_set_or_map(ann, binding_name, semantic)
                        }),
                    _ => false,
                })
        }
        _ => false,
    }
}

fn is_set_or_map_constructor(new_expr: &NewExpression<'_>) -> bool {
    matches!(
        &new_expr.callee,
        Expression::Identifier(id) if matches!(id.name.as_str(), "Set" | "Map")
    )
}

/// True when a TS type is a built-in generic `Set<…>` / `Map<…>` reference —
/// the collections whose `.has()` is O(1). The built-ins are always applied to
/// type arguments, so a reference without them (a bare, non-generic
/// user-declared `Set`/`Map` alias) is not matched. Parenthesized types
/// (`(Set<string>)`) are unwrapped.
fn type_is_set_or_map(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeReference(type_ref) => {
            type_ref.type_arguments.is_some()
                && matches!(
                    &type_ref.type_name,
                    TSTypeName::IdentifierReference(id)
                        if matches!(id.name.as_str(), "Set" | "Map")
                )
        }
        TSType::TSParenthesizedType(p) => type_is_set_or_map(&p.type_annotation),
        _ => false,
    }
}

/// True when an object-type member list declares `binding_name` as a `Set<…>` /
/// `Map<…>` property — the shape behind a destructured params object
/// (`{ excludedColumns }: { excludedColumns?: Set<string> }`).
fn members_declare_set_or_map(members: &[TSSignature], binding_name: &str) -> bool {
    members.iter().any(|member| match member {
        TSSignature::TSPropertySignature(prop) => {
            prop.key.static_name().as_deref() == Some(binding_name)
                && prop
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_is_set_or_map(&ann.type_annotation))
        }
        _ => false,
    })
}

/// True when a named type reference (`WriteRowsOptions` in `{ excludedColumns }:
/// WriteRowsOptions`) resolves to a `type`/`interface` declaration whose
/// `binding_name` member is a `Set<…>` / `Map<…>` property. The declaration is
/// matched by name across the module — the established resolution shape in this
/// codebase (cf. `no_array_callback_reference`'s `named_type_member_is_low_arity`).
/// A reference carrying its own type arguments is skipped: the member type may
/// depend on a type parameter not statically visible here.
fn named_type_member_is_set_or_map<'a>(
    type_ref: &TSTypeReference<'a>,
    binding_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if type_ref.type_arguments.is_some() {
        return false;
    }
    let TSTypeName::IdentifierReference(id) = &type_ref.type_name else {
        return false;
    };
    let type_name = id.name.as_str();
    semantic.nodes().iter().any(|node| match node.kind() {
        AstKind::TSTypeAliasDeclaration(alias) if alias.id.name.as_str() == type_name => {
            matches!(&alias.type_annotation, TSType::TSTypeLiteral(lit)
                if members_declare_set_or_map(&lit.members, binding_name))
        }
        AstKind::TSInterfaceDeclaration(iface) if iface.id.name.as_str() == type_name => {
            members_declare_set_or_map(&iface.body.body, binding_name)
        }
        _ => false,
    })
}

/// True when a parameter's type annotation makes `binding_name` a `Set<…>` /
/// `Map<…>`. Covers a direct annotation on the parameter itself
/// (`s: Set<string>`), the destructured inline-object case
/// (`{ s }: { s: Set<string> }`), and the destructured named-type case
/// (`{ excludedColumns }: WriteRowsOptions`).
fn param_binding_is_set_or_map<'a>(
    ann: &TSTypeAnnotation<'a>,
    binding_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Direct annotation on the parameter itself (`(s: Set<string>) => …`).
    if type_is_set_or_map(&ann.type_annotation) {
        return true;
    }
    // Destructured from a typed params object.
    match &ann.type_annotation {
        TSType::TSTypeLiteral(lit) => members_declare_set_or_map(&lit.members, binding_name),
        TSType::TSTypeReference(type_ref) => {
            named_type_member_is_set_or_map(type_ref, binding_name, semantic)
        }
        _ => false,
    }
}

/// Methods that exist ONLY on `String.prototype` and return a `string`. A call
/// to one of these is statically a string-typed expression, so a chained
/// `.includes()`/`.indexOf()` is a substring search rather than array membership.
/// Array-shared names (`slice`, `concat`, `toString`) are deliberately excluded:
/// matching on the method name alone can't tell `str.slice()` from `arr.slice()`,
/// and exempting `arr.slice().includes(x)` would silently miss a real O(n*m) scan.
const STRING_RETURNING_METHODS: &[&str] = &[
    "toLowerCase",
    "toUpperCase",
    "trim",
    "trimStart",
    "trimEnd",
    "substring",
    "substr",
    "replace",
    "replaceAll",
    "normalize",
    "padStart",
    "padEnd",
    "repeat",
    "charAt",
];

/// True when `expr` is statically a `string`: a string literal/template, or a
/// call to a string-returning method (`.toLowerCase()`, `.trim()`, …). Used to
/// skip `String.prototype.includes`/`indexOf` substring searches, which have no
/// collection to replace with a Map/Set.
fn receiver_is_string(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::CallExpression(call) => matches!(
            &call.callee,
            Expression::StaticMemberExpression(member)
                if STRING_RETURNING_METHODS.contains(&member.property.name.as_str())
        ),
        _ => false,
    }
}

/// True when `expr` is an identifier bound to a `const` declaration whose
/// initializer is statically a string (`receiver_is_string`: a string
/// literal/template or a string-returning call like `component.toLowerCase()`).
/// Such a binding names a string one binding removed
/// (`const lc = s.toLowerCase(); lc.includes(x)`), so `.includes()`/`.indexOf()`
/// on it is a `String.prototype` substring search — the same category as the
/// inline `s.toLowerCase().includes(x)` form — with no collection to hash into a
/// Map/Set. Only `const` bindings are followed: a `let`/`var` could be reassigned
/// to a non-string, so its static type is not guaranteed.
fn receiver_is_const_bound_string<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(id) = expr else {
        return false;
    };
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const {
        return false;
    }
    matches!(&decl.init, Some(init) if receiver_is_string(init))
}

/// True when the `.includes()`/`.indexOf()` receiver is an identifier proven to
/// be a `string` by a `typeof <ident> === "string"` guard in an enclosing `&&`
/// chain that lexically precedes the use
/// (`typeof preset === "string" && preset.includes(word)`). The `&&` short-circuit
/// guarantees the guard held when the call ran, so the call is a substring search,
/// not array membership. The walk stops at a function boundary: beyond it the
/// short-circuit no longer dominates the use (a nested closure runs later).
fn receiver_is_typeof_narrowed_string<'a>(
    receiver: &Expression<'_>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(recv) = receiver else {
        return false;
    };
    let name = recv.name.as_str();

    let nodes = semantic.nodes();
    let mut child_span = node.kind().span();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::LogicalExpression(logical)
                if logical.operator == LogicalOperator::And
                    && logical.right.span().contains_inclusive(child_span)
                    && conjunction_narrows_to_string(&logical.left, name) =>
            {
                return true;
            }
            _ => {}
        }
        child_span = ancestor.kind().span();
    }
    false
}

/// True when `expr`, read as a conjunction (a nested `&&` chain, a parenthesised
/// sub-expression, or a single comparison), contains a `typeof <name> ===
/// "string"` guard. Recurses only through `&&` and parentheses — positions where
/// every conjunct must hold, preserving the narrowing.
fn conjunction_narrows_to_string(expr: &Expression<'_>, name: &str) -> bool {
    match expr {
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::And => {
            conjunction_narrows_to_string(&logical.left, name)
                || conjunction_narrows_to_string(&logical.right, name)
        }
        Expression::ParenthesizedExpression(paren) => {
            conjunction_narrows_to_string(&paren.expression, name)
        }
        Expression::BinaryExpression(bin) => is_typeof_string_eq(bin, name),
        _ => false,
    }
}

/// True when `bin` is `typeof <name> === "string"`, accepting either operand
/// order (`typeof x === "string"` and `"string" === typeof x`).
fn is_typeof_string_eq(bin: &BinaryExpression<'_>, name: &str) -> bool {
    if bin.operator != BinaryOperator::StrictEquality {
        return false;
    }
    (is_typeof_of_name(&bin.left, name) && is_string_type_tag(&bin.right))
        || (is_typeof_of_name(&bin.right, name) && is_string_type_tag(&bin.left))
}

/// True when `expr` is `typeof <name>` for the given identifier name.
fn is_typeof_of_name(expr: &Expression<'_>, name: &str) -> bool {
    matches!(
        expr,
        Expression::UnaryExpression(unary)
            if unary.operator == UnaryOperator::Typeof
                && matches!(&unary.argument, Expression::Identifier(id) if id.name.as_str() == name)
    )
}

/// True when `expr` is the string literal `"string"` (the `typeof` tag a string
/// value produces).
fn is_string_type_tag(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::StringLiteral(lit) if lit.value.as_str() == "string")
}

/// True when `call`'s callback is invoked once per element of the receiver
/// (`.forEach`/`.map`/`.filter`/`.find`/…), so the rule treats that callback as
/// a loop body.
fn call_iterates_via_callback(call: &CallExpression<'_>) -> bool {
    matches!(
        &call.callee,
        Expression::StaticMemberExpression(member)
            if CALLBACK_ITERATING_METHODS.contains(&member.property.name.as_str())
    )
}

fn is_inside_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // `child` is the node we ascended from on each step — the subtree of the
    // current ancestor that contains `node`. It distinguishes an iterator
    // method's per-iteration callback subtree from its receiver subtree.
    let mut child = nodes.get_node(node.id());
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,

            // `for..of` / `for..in`: a call in the ITERABLE expression
            // (`for (const x of <HERE>)`) runs once before the loop, not per
            // iteration, so it is not an O(n*m) site for THIS loop — only the
            // BODY repeats. When we ascended from the iterable subtree, keep
            // walking to catch an OUTER loop that would repeat the whole `for..of`.
            AstKind::ForOfStatement(for_of) => {
                if child.kind().span() != for_of.right.span() {
                    return true;
                }
            }
            AstKind::ForInStatement(for_in) => {
                if child.kind().span() != for_in.right.span() {
                    return true;
                }
            }

            // Named function/class/method boundaries — hoisted definitions
            // don't necessarily execute per iteration.
            AstKind::Function(f) if f.id.is_some() => return false,
            AstKind::Class(_) => return false,

            // Arrow / anonymous-function boundaries stop the walk: a callback
            // passed to an ordinary call (`bench(...)`/`group(...)`) does not run
            // per enclosing-loop iteration. The exception is a callback that
            // iterates (`.forEach`/`.map`/`.filter`/…), which IS a loop body —
            // leave the walk to the `CallExpression` arm below.
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
                    && call_iterates_via_callback(call)
                {
                    child = ancestor;
                    continue;
                }
                return false;
            }

            // A callback-iterating method (`.forEach`/`.map`/`.filter`/…) is a
            // loop body only for its callback. When we arrived through the callee
            // (`X.map` member-expression receiver chain), `node` is a downstream
            // stage of a sequential pipeline (`a.filter(…).map(…)`) that runs
            // once, not per iteration — keep walking up.
            AstKind::CallExpression(call) => {
                if call_iterates_via_callback(call)
                    && !call.callee.span().contains_inclusive(child.kind().span())
                {
                    return true;
                }
            }

            _ => {}
        }
        child = ancestor;
    }
    false
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
    fn flags_find_in_for_loop() {
        let diags = run(r#"
for (const item of items) {
    const match = others.find(o => o.id === item.id);
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".find()"));
    }

    #[test]
    fn flags_find_in_for_statement() {
        let diags = run(r#"
for (let i = 0; i < items.length; i++) {
    const m = arr.findIndex(x => x.id === items[i].id);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_filter_in_while() {
        let diags = run(r#"
while (hasMore) {
    const filtered = items.filter(i => i.id === target);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_find_in_foreach() {
        let diags = run(r#"
items.forEach(item => {
    const match = others.find(o => o.id === item.id);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_find_in_map() {
        let diags = run(r#"
const result = items.map(item => {
    return categories.find(c => c.id === item.categoryId);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_find_outside_loop() {
        assert!(
            run(r#"
const user = users.find(u => u.id === targetId);
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_map_without_find() {
        assert!(
            run(r#"
const names = items.map(i => i.name);
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_on_non_loop_call() {
        assert!(
            run(r#"
function process() {
    const item = arr.find(x => x.id === id);
    return item;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_in_anon_callback_to_ordinary_call_inside_loop() {
        // Regression for #3844: `bench(...)`/`group(...)` are ordinary calls, not
        // iterator methods — their callbacks are not run per loop iteration, and
        // `router.find()` here is a MedleyRouter method, not Array.prototype.find.
        assert!(
            run(r#"
for (const benchRoute of benchRoutes) {
    group(`${benchRoute.method} ${benchRoute.path}`, () => {
        bench('MedleyRouter', () => {
            const router = new MedleyRouter();
            const match = router.find(benchRoute.path);
            match.store[benchRoute.method];
        });
    });
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_in_named_function_inside_loop() {
        assert!(
            run(r#"
items.forEach(item => {
    function helper() { return others.find(o => o.id === id); }
    return helper;
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_relation_property_receiver() {
        // Regression for #757: product.correspondences is a bounded relation field.
        assert!(
            run(r#"
const fields = centrales.flatMap((centrale) => {
    const corr = product.correspondences.find((c) => c.centraleId === centrale.id) ?? null;
    return corr;
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_nested_member_chain() {
        // a.b.c is still a member expression — should not be flagged.
        assert!(
            run(r#"
items.forEach(item => {
    const x = a.b.c.find(v => v.id === item.id);
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_set_has_lookup_in_filter() {
        // Regression for #957: updatedGtins is a Set — `.has()` is already O(1).
        assert!(
            run(r#"
const updatedGtins = new Set(updatedRows.map((r) => r.gtin));
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.has(r.gtin))
  .map((r) => r.gtin);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_map_has_lookup_in_find_inside_loop() {
        assert!(
            run(r#"
const byId = new Map(items.map((i) => [i.id, i]));
for (const row of rows) {
    const known = candidates.find((c) => byId.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_direct_new_set_has_receiver() {
        assert!(
            run(r#"
const unknown = parsedRows
  .filter((r) => !new Set(updatedGtins).has(r.gtin))
  .map((r) => r.gtin);
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_includes_lookup_in_filter_chain() {
        // Plain-array `.includes()` is the genuine O(n*m) pattern.
        let diags = run(r#"
const updatedGtins = updatedRows.map((r) => r.gtin);
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.includes(r.gtin))
  .map((r) => r.gtin);
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn no_fp_on_string_literal_includes_in_loop() {
        // Regression for #3730: `fullPath` is a string, `.includes("figma")` is a
        // substring search — there is no array to index into a Map/Set.
        assert!(
            run(r#"
for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (fullPath.includes("figma")) continue;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_literal_index_of_in_loop() {
        assert!(
            run(r#"
for (const s of strings) {
    const i = s.indexOf("/");
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_receiver_to_lower_case_includes_in_loop() {
        // Regression for #4566: `team.name.toLowerCase().includes(query)` is a
        // case-insensitive substring filter — the receiver is a string, not an
        // array, so there is no collection to index into a Map/Set.
        assert!(
            run(r#"
for (const team of teams) {
    if (team.name.toLowerCase().includes(normalizedQuery)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_literal_receiver_includes_variable_arg_in_loop() {
        // Regression for #4566: `"abc".includes(x)` is a substring search even
        // when the argument is a variable.
        assert!(
            run(r#"
for (const x of xs) {
    if ("abc".includes(x)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_template_literal_receiver_includes_in_loop() {
        assert!(
            run(r#"
for (const x of xs) {
    if (`prefix-${x}`.includes(needle)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_array_slice_includes_in_loop() {
        // `slice`/`concat` exist on `Array.prototype` too — matching the method
        // name alone must not exempt a genuine array-membership scan.
        let diags = run(r#"
for (const r of rows) {
    if (bigArray.slice(0, 100).includes(r.gtin)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_array_concat_includes_in_loop() {
        let diags = run(r#"
for (const r of rows) {
    if (bigArray.concat(extra).includes(r.gtin)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_includes_with_variable_arg_in_loop() {
        // Array-membership with a variable argument is the genuine O(n*m) scan.
        let diags = run(r#"
for (const r of rows) {
    if (updatedGtins.includes(r.gtin)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_includes_with_identifier_arg_in_loop() {
        let diags = run(r#"
for (const k of keys) {
    if (allowed.includes(k)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_has_on_unknown_receiver() {
        // `updatedGtins` is not provably a Set/Map — keep flagging the `.find`
        // that runs per loop iteration.
        let diags = run(r#"
const updatedGtins = getGtins();
for (const row of rows) {
    const known = candidates.find((c) => updatedGtins.has(c.id));
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_has_on_reassigned_receiver() {
        // The binding is reassigned after the Set declaration — no guarantee left.
        let diags = run(r#"
let updatedGtins = new Set(getGtins());
updatedGtins = getGtins();
for (const row of rows) {
    const known = candidates.find((c) => updatedGtins.has(c.id));
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_find_callback_over_plain_array_inside_loop() {
        let diags = run(r#"
for (const item of items) {
    const match = others.find((o) => candidates.find((c) => c.id === o.id));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_call_expression_receiver() {
        // getCategories() is a call result — unbounded, should still be flagged.
        let diags = run(r#"
items.map(item => {
    return getCategories().find(c => c.id === item.categoryId);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_filter_as_map_receiver() {
        // Regression for #3784: `.filter()` is the receiver of `.map()`, a
        // sequential pipeline stage that runs once — not a per-iteration body.
        assert!(
            run(r#"
const out = files.filter((f) => f.isDirectory()).map((f) => f.name);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_longer_pipeline_chain() {
        assert!(
            run(r#"
const r = files.filter((a) => a.ok).map((b) => b.id).filter((c) => !c.hidden);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_filter_then_foreach() {
        assert!(
            run(r#"
arr.filter((x) => x.ok).forEach((y) => use(y));
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_filter_in_map_callback() {
        // The inner `.filter` is nested in the `.map` callback — per-iteration.
        let diags = run(r#"
const r = items.map((i) => others.filter((o) => o.id === i.id));
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_two_arg_index_of_cursor_in_loop() {
        // Regression for #4529: `indexOf('}', i + 3)` is a forward-scan cursor
        // (string positional walk), not an array-membership lookup.
        assert!(
            run(r#"
for (let i = 0; i < n; i++) {
    rawIndex = rawTemplate.indexOf('}', rawIndex + 3);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_two_arg_index_of_variable_search_in_loop() {
        // A 2-arg `indexOf(value, fromIndex)` is a positional scan regardless of
        // whether the search value is a literal or a variable.
        assert!(
            run(r#"
for (const x of xs) {
    const j = arr.indexOf(x, 5);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_one_arg_index_of_membership_in_loop() {
        // A 1-arg `indexOf(value)` membership test is the genuine O(n*m) scan —
        // only the 2-arg `(value, fromIndex)` cursor form is exempt.
        let diags = run(r#"
for (const item of list) {
    if (bigList.indexOf(item) !== -1) { found.push(item); }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_inline_literal_array_includes_in_loop() {
        // Regression for #4490: `["./", "/"].includes(slug)` — the receiver is an
        // inline literal array of fixed size 2, so the scan is O(constant) = O(1).
        assert!(
            run(r#"
for (const o of outputs) { if (["./", "/"].includes(o.slug)) {} }
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_inline_literal_array_find_in_loop() {
        // A literal array is fixed-size for every lookup method, not just includes.
        assert!(
            run(r#"
for (const x of items) { const m = [1, 2, 3].find(v => v === x); }
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_chain_rooted_in_literal_array_flat_filter_in_loop() {
        // Regression for #6612: `[lockFile, files].flat().filter(Boolean)` — the
        // root of the chain is an inline 2-element array literal, so the scan is
        // O(constant); intermediate `.flat()` stays bounded by that fixed size.
        assert!(
            run(r#"
for (const packageManager of packageManagers) {
    const detectionsFiles = [packageManager.lockFile, packageManager.files]
        .flat()
        .filter(Boolean);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_chain_rooted_in_literal_array_slice_find_in_loop() {
        // `.slice(0, n)` between the literal and the lookup is also a bounded
        // transform of a fixed-size array.
        assert!(
            run(r#"
for (const x of items) {
    const m = [1, 2, 3, 4].slice(0, 2).find(v => v === x);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_direct_literal_array_filter_in_loop() {
        // The base case of the chain walk: `.filter()` directly on a literal array.
        assert!(
            run(r#"
for (const x of items) {
    const m = [1, 2, 3].filter(v => v === x);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_chain_rooted_in_param_flat_filter_in_loop() {
        // The chain root is a parameter (unbounded), not a literal array — the
        // intermediate `.flat()` does not bound it, so still flagged.
        let diags = run(r#"
function process(arr) {
    for (const x of items) {
        const m = arr.flat().filter(v => v === x);
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_bare_unbounded_filter_in_loop() {
        let diags = run(r#"
for (const x of items) {
    const m = collection.filter(v => v === x);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_variable_receiver_includes_in_loop() {
        // A variable receiver is a collection that can grow with input — flagged.
        let diags = run(r#"
for (const o of outputs) { if (bigList.includes(o.slug)) {} }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_variable_receiver_find_in_loop() {
        let diags = run(r#"
for (const x of items) { const m = collection.find(v => v === x); }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_lookup_in_for_of_iterable() {
        // Regression for #4491: `.filter()` in the ITERABLE of `for..of` runs once
        // before the loop, not per iteration — not an O(n*m) site for this loop.
        assert!(
            run(r#"
for (const output of outputs.filter((o) => !o.type)) { const x = output.file; }
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_lookup_in_for_of_iterable_find() {
        // A lookup in the `for..of` iterable runs once for any lookup method.
        assert!(
            run(r#"
for (const x of arr.find(p => p.ok)) { use(x); }
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_lookup_in_for_of_body() {
        // A `.filter()` in the loop BODY runs per iteration — still flagged.
        let diags = run(r#"
for (const x of items) { const m = list.filter(v => v === x); }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_for_of_iterable_lookup_inside_outer_loop() {
        // The inner `for..of` iterable lookup runs once per OUTER-loop iteration —
        // the ascent must still reach the outer loop and flag it.
        let diags = run(r#"
for (const o of outer) { for (const x of inner.filter(p => p.id === o.id)) { use(x); } }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_const_bound_inline_array_includes_in_loop() {
        // Regression for #6623: `validValues` is a `const` bound to a fixed
        // 2-element array literal declared in the loop body — O(constant), the
        // inline `["yes", "no"].includes(x)` form one binding removed.
        assert!(
            run(r#"
for (const item of xmlItems) {
    const validValues = ['yes', 'no'];
    if (validValues.includes(item.family_friendly)) { keep(item); }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_const_bound_four_element_array_includes_in_loop() {
        // A larger but still statically-fixed const array is equally bounded.
        assert!(
            run(r#"
for (const price of prices) {
    const validTypes = ['rent', 'purchase', 'package', 'subscription'];
    if (!validTypes.includes(price.type)) { reject(price); }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_const_bound_empty_array_accumulator_in_loop() {
        // `const seen = []` is a growing accumulator (`seen.push(x)`) — the
        // genuine O(n*m) membership scan the rule targets. The empty-array init
        // must NOT be exempted.
        let diags = run(r#"
const seen = [];
for (const x of xs) {
    if (seen.includes(x)) { continue; }
    seen.push(x);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_let_bound_inline_array_includes_in_loop() {
        // A `let` binding could be reassigned to a larger array — the size is not
        // statically bounded, so it is still flagged.
        let diags = run(r#"
for (const item of items) {
    let validValues = ['yes', 'no'];
    if (validValues.includes(item.flag)) { keep(item); }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_param_receiver_includes_in_loop() {
        // A function-parameter receiver is unbounded — not bound to a literal
        // array declaration — so it is still flagged.
        let diags = run(r#"
function check(validValues) {
    for (const item of items) {
        if (validValues.includes(item.flag)) { keep(item); }
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_typeof_string_narrowed_receiver_includes() {
        // Regression for #6357: `preset` is narrowed to `string` by the
        // `typeof preset === 'string'` guard in the same `&&`, so
        // `preset.includes(word)` is a substring search, not array membership.
        assert!(
            run(r#"
const matched = KEYWORDS_EDGE_TARGETS.some(
    word =>
        (typeof preset === "string" && preset.includes(word))
        || process.env.NITRO_PRESET?.includes(word),
);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_typeof_string_narrowed_receiver_reversed_operand_order() {
        // The `===` operands may appear in either order.
        assert!(
            run(r#"
const matched = words.some(
    word => "string" === typeof preset && preset.includes(word),
);
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_genuine_array_includes_without_typeof_guard() {
        // No `typeof x === "string"` guard — a genuine array-membership scan in a
        // loop is still the O(n*m) pattern the rule targets.
        let diags = run(r#"
const seen = getSeen();
for (const x of xs) {
    if (seen.includes(x)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_includes_when_typeof_guard_is_for_different_identifier() {
        // The `typeof other === 'string'` guard narrows `other`, not the receiver
        // `x`, so `x.includes(y)` is unaffected and still flagged.
        let diags = run(r#"
for (const y of ys) {
    if (typeof other === "string" && x.includes(y)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_bare_named_predicate_filter_in_loop() {
        // Regression for #7211: a bare named predicate is an opaque callback with
        // no visible membership lookup — there is no collection to pre-index.
        assert!(
            run(r#"
for (const item of items) {
    const valid = res.filter(filterValidExtends);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_literal_only_side_effecting_filter_callback_in_loop() {
        // Regression for #7211: the callback only compares its parameter to string
        // literals and performs a side effect — nothing to hash into a Map/Set.
        assert!(
            run(r#"
for (const attr of attrs) {
    const kept = modifiers.filter((m) => {
        if (m === 'capture') { append(m); return false; }
        return true;
    });
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_property_truthiness_filter_callback_in_loop() {
        // Regression for #7211: a plain property-truthiness callback performs no
        // membership/equality scan of a captured collection.
        assert!(
            run(r#"
for (const x of xs) {
    const active = arr.filter((r) => r.active);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_filter_callback_with_inner_includes_against_captured_collection() {
        // The callback scans `rootVars` (captured) via `.includes()` — a genuine
        // O(n*m) site a Set could replace. Both the `.filter()` and the inner
        // `.includes()` (itself a per-iteration membership scan) are flagged.
        let diags = run(r#"
for (const decl of decls) {
    const free = scopes.filter((name) => !rootVars.includes(name));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_find_callback_with_inner_includes_and_destructured_param() {
        // A destructured param (`{ type }`) with an inner `.includes()` against a
        // captured collection is still the O(n*m) pattern.
        let diags = run(r#"
for (const parent of parents) {
    const bad = nodes.find(({ type }) => !TS_NODE_TYPES.includes(type));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_find_callback_with_equality_against_captured_key() {
        // `x.id === key` compares the element against a captured `key` — a Map
        // keyed by id could replace the linear scan.
        let diags = run(r#"
for (const item of items) {
    const hit = arr.find((x) => x.id === key);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_const_bound_string_call_receiver_includes_in_loop() {
        // Regression for #7420: `lc` is a `const` bound to `component.toLowerCase()`,
        // a string-returning call, so `lc.includes(keyword)` is a substring search —
        // the same category as the inline `s.toLowerCase().includes(x)` form, with no
        // collection to index into a Map/Set.
        assert!(
            run(r#"
function getMatchedPackage(component) {
    const lc = component.toLowerCase();
    for (const pkgConfig of lazyPackages) {
        const keyword = pkgConfig.name.split('/').pop();
        if (lc.includes(keyword)) { return pkgConfig; }
    }
    return null;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_const_bound_string_literal_receiver_includes_in_loop() {
        // A `const` bound to a string literal is statically a string one binding
        // removed, so `.includes()` on it is a substring search.
        assert!(
            run(r#"
const s = 'prefix';
for (const x of xs) {
    if (s.includes(x)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_const_bound_non_string_call_receiver_includes_in_loop() {
        // A `const` bound to a plain function call (`getKeywords()`) is NOT provably
        // a string — its result may be an array — so the membership scan is still the
        // genuine O(n*m) pattern the rule targets.
        let diags = run(r#"
const arr = getKeywords();
for (const x of xs) {
    if (arr.includes(x)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_let_bound_string_receiver_includes_in_loop() {
        // A `let` binding could be reassigned to a non-string after the string init,
        // so its static type is not guaranteed — only `const` bindings are followed.
        let diags = run(r#"
let s = 'prefix';
for (const x of xs) {
    if (s.includes(x)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_array_param_membership_in_loop() {
        // #7420 control: `customKeywords` is a function parameter (an unbounded
        // array) — a genuine array-membership scan the rule targets, unaffected by
        // the const-bound string exemption.
        let diags = run(r#"
function scan(customKeywords) {
    for (const k of ks) {
        if (customKeywords.includes(k)) {}
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_optional_chained_has_typed_set_param_destructured() {
        // Regression for #7548: `excludedColumns` is a `Set<string>` destructured
        // from a typed params object and `?.has()` is O(1), so the `.filter()`
        // over the column names is not the O(n*m) scan the rule targets. Combines
        // both fixes — the typed-binding recognition and the ChainExpression unwrap.
        assert!(
            run(r#"
interface WriteRowsOptions {
    excludedColumns?: Set<string>;
}
function writeRows({ excludedColumns }: WriteRowsOptions) {
    for (const batch of batches) {
        const names = Object.keys(batch).filter(
            (columnName) => !excludedColumns?.has(columnName),
        );
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_map_typed_param_via_type_alias() {
        // #7548: a `Map<…>` destructured from a `type` alias resolves the same way
        // as the interface case — `.has()` on it is O(1).
        assert!(
            run(r#"
type Ctx = { seen: Map<string, number> };
function scan({ seen }: Ctx) {
    for (const row of rows) {
        const hit = candidates.find((c) => seen.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_inline_object_type_destructured_set_param() {
        // #7548: a member destructured from an INLINE object type
        // (`{ s }: { s: Set<string> }`) is a known Set via the type-literal arm.
        assert!(
            run(r#"
function scan({ s }: { s: Set<string> }) {
    for (const row of rows) {
        const kept = candidates.filter((c) => s.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_variable_annotated_set_in_loop() {
        // #7548: a `const s: Set<string>` variable annotation identifies a known
        // Set even without a `new Set()` initializer, so `s.has()` is O(1).
        assert!(
            run(r#"
const s: Set<string> = getSet();
for (const row of rows) {
    const kept = candidates.filter((c) => s.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_typed_set_param_non_optional() {
        // #7548 gap-1-only: a `Set<string>` parameter with a plain (non-optional)
        // `.has()` is O(1) — the typed-binding recognition alone must exempt it.
        assert!(
            run(r#"
function scan(excluded: Set<string>) {
    for (const row of rows) {
        const kept = candidates.filter((c) => !excluded.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_optional_chained_has_local_new_set() {
        // #7548 gap-2-only: an optional-chained `?.has()` on a local `new Set()`
        // binding — the ChainExpression unwrap alone must reach the known-Set
        // receiver behind the `?.`.
        assert!(
            run(r#"
const seen = new Set(getIds());
for (const row of rows) {
    const kept = candidates.filter((c) => !seen?.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_includes_on_typed_string_array_param_in_loop() {
        // #7548 negative space: `bigArray` is a `string[]` — `.includes()` is the
        // genuine O(n*m) scan a Set would replace, and a `string[]` annotation must
        // not be mistaken for a Set/Map.
        let diags = run(r#"
function scan(bigArray: string[]) {
    for (const row of rows) {
        const kept = candidates.filter((x) => bigArray.includes(x.id));
    }
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_has_on_untyped_unknown_binding_in_loop() {
        // #7548 negative space: `obj` has neither a `new Set()` init nor a Set/Map
        // type annotation — it is not provably a Set, so the `.filter()` that runs
        // per loop iteration stays flagged.
        let diags = run(r#"
function scan(obj) {
    for (const row of rows) {
        const kept = candidates.filter((c) => obj.has(c.id));
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }
}
