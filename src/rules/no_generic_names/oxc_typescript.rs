//! no-generic-names OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const BANNED_WORDS: &[&str] = &[
    "info", "temp", "result", "obj", "item", "thing", "stuff", "val", "retval", "value", "foo",
    "bar", "row", "rows", "cell", "cells",
];

const PARAM_ALLOWED_WORDS: &[&str] = &["value", "item"];

const BANNED_PREFIXES: &[&str] = &["process", "data", "do", "execute", "run", "perform"];

const GLOBAL_IDENTIFIER_ALLOWLIST: &[&str] = &[
    "process",
    "require",
    "module",
    "exports",
    "Buffer",
    "globalThis",
    "console",
    "__dirname",
    "__filename",
];

const DESCRIPTIVE_SUFFIXES: &[&str] = &[
    "_DIR", "_PATH", "_FILE", "_URL", "_URI", "_KEY", "_ID", "_PORT", "_HOST", "_ADDR", "_SIZE",
    "_LEN", "_COUNT", "_MAX", "_MIN", "_TIMEOUT", "_INTERVAL", "_LIMIT", "_TTL", "_ROOT", "_BASE",
];

/// camelCase/PascalCase suffixes that turn a banned prefix into a specific
/// domain noun rather than a generic action. `run` is a generic verb but
/// `runId`/`RunStatus` name a concrete run identifier/state; `data` is filler
/// but `dataType`/`dataKey` name a concrete field. The suffix must sit on a
/// camelCase boundary at the end of the name, mirroring the SCREAMING_SNAKE_CASE
/// `_ID`/`_KEY` exemption in `DESCRIPTIVE_SUFFIXES`.
const DESCRIPTIVE_CAMEL_SUFFIXES: &[&str] = &["Id", "Status", "Type", "Key", "Json", "At"];

/// Number of capitalized word segments in `suffix` (each uppercase letter
/// preceded by a non-uppercase character, or at position 0, starts a segment).
/// `BeforeUnload` → 2, `CommandOnSnapshot` → 3, `Task` → 1, `NpmAudit` → 2.
fn capitalized_word_count(suffix: &str) -> usize {
    let bytes = suffix.as_bytes();
    bytes
        .iter()
        .enumerate()
        .filter(|(i, b)| {
            b.is_ascii_uppercase() && (*i == 0 || !bytes[i - 1].is_ascii_uppercase())
        })
        .count()
}

/// True when `name` ends with one of `DESCRIPTIVE_CAMEL_SUFFIXES` on a camelCase
/// boundary (the character preceding the suffix is lowercase or a digit). This
/// keeps generic verbs flagged (`runTask`, `dataSource`) while exempting
/// domain nouns (`runId`, `runFriendlyId`, `RunStatus`, `dataType`).
fn ends_with_descriptive_camel_suffix(name: &str) -> bool {
    DESCRIPTIVE_CAMEL_SUFFIXES.iter().any(|suffix| {
        name.strip_suffix(suffix).is_some_and(|head| {
            head.bytes()
                .next_back()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
    })
}

/// PascalCase `Data*` compound names exempted from the `data` banned-prefix
/// check: UI primitives (shadcn/TanStack/MUI/Radix) and fp-ts/Effect
/// data-first/data-last vocabulary (`dual()` pattern, `DataTag` discriminants).
const DATA_PASCAL_CASE_ALLOWED_COMPOUNDS: &[&str] = &[
    "DataTable",
    "DataGrid",
    "DataView",
    "DataList",
    "DataLast",
    "DataFirst",
    "DataTag",
];

/// Standard DOM / Web API type names whose camelCase form is the conventional
/// identifier for a value of that type. `dataTransfer: DataTransfer` mirrors
/// `DragEvent.dataTransfer`; `dataUrl`/`dataURL` name an RFC 2397 `data:` scheme
/// URL, mirroring `FileReader.readAsDataURL`. Each name specifically refers to
/// the web platform value, so it is not a generic `data*` compound. Matched by
/// exact, case-sensitive full name.
const DOM_API_NAME_ALLOWLIST: &[&str] = &["dataTransfer", "dataUrl", "dataURL"];

/// True when `name` exactly mirrors a standard DOM/Web API type name in
/// camelCase (e.g. `dataTransfer` for `DataTransfer`).
fn mirrors_dom_api_name(name: &str) -> bool {
    DOM_API_NAME_ALLOWLIST.contains(&name)
}

/// Grafana plugin-SDK base classes that a datasource plugin entry extends. The
/// SDK mandates the implementing class be named exactly `DataSource`, so a class
/// extending one of these is a framework-prescribed entry point, not a lazily
/// named `data*` compound.
const GRAFANA_DATASOURCE_BASES: &[&str] = &["DataSourceApi", "DataSourceWithBackend"];

/// True when `node` is the name of a class that `extends` a Grafana datasource
/// SDK base (`class DataSource extends DataSourceApi<…>`). The exemption is
/// scoped to the class *name* binding: the node must be the class's own `id`, so
/// generic identifiers declared inside the class body stay flagged. Matches both
/// a bare base (`DataSourceApi`) and a namespaced one (`grafana.DataSourceApi`)
/// by its last `.`-segment, mirroring the heritage check in `react-no-typos`.
fn is_grafana_datasource_class_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    ctx: &CheckCtx,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let AstKind::Class(class) = nodes.kind(parent_id) else {
        return false;
    };
    // The node must be the class's own name, not a member inside its body.
    let node_span = node.kind().span();
    if class.id.as_ref().is_none_or(|id| id.span != node_span) {
        return false;
    }
    let Some(super_class) = &class.super_class else {
        return false;
    };
    let start = super_class.span().start as usize;
    let end = super_class.span().end as usize;
    if end > ctx.source.len() {
        return false;
    }
    let base = &ctx.source[start..end];
    let base = base.rsplit('.').next().unwrap_or(base);
    GRAFANA_DATASOURCE_BASES.contains(&base)
}

/// Return the banned prefix matching `name` on a word boundary, or None.
fn matched_banned_prefix(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    for &prefix in BANNED_PREFIXES {
        let plen = prefix.len();
        if bytes.len() < plen {
            continue;
        }
        if !bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
            continue;
        }
        let on_boundary = if bytes.len() == plen {
            true
        } else if bytes[..plen].iter().all(|b| b.is_ascii_uppercase()) {
            if bytes[plen] != b'_' {
                continue;
            }
            let suffix = &name[plen..];
            if DESCRIPTIVE_SUFFIXES
                .iter()
                .any(|s| suffix.eq_ignore_ascii_case(s))
            {
                continue;
            }
            true
        } else {
            bytes[plen].is_ascii_uppercase() || bytes[plen] == b'_'
        };
        if on_boundary {
            // `runWith*` is the idiomatic AsyncLocalStorage wrapper pattern —
            // the `run` comes from `AsyncLocalStorage.run()`, not a generic verb.
            if prefix == "run" && name[plen..].starts_with("With") {
                continue;
            }
            // A `run`-prefixed compound whose suffix names *what* is run with
            // ≥2 capitalized words (`runBeforeUnload`, `runNpmAudit`) is
            // self-documenting; only single-word fillers (`runTask`) stay generic.
            if prefix == "run" && capitalized_word_count(&name[plen..]) >= 2 {
                continue;
            }
            // A domain-identifying suffix (`runId`, `RunStatus`, `dataType`)
            // makes the compound a specific noun, not a generic action.
            if ends_with_descriptive_camel_suffix(name) {
                continue;
            }
            return Some(prefix);
        }
    }
    None
}

/// True when the node is inside a destructuring pattern (object pattern).
fn is_destructuring<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::ObjectPattern(_)) {
            return true;
        }
        // Stop at statement boundaries
        if matches!(
            kind,
            AstKind::VariableDeclaration(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_)
        ) {
            break;
        }
    }
    false
}

/// True when `expr` is (or unwraps to) a `.entries()` method call. Matched by the
/// callee's member property name `entries`, covering `Object.entries(obj)`,
/// `map.entries()`, and `arr.entries()` regardless of receiver. Computed/static
/// member access on top of the call is unwrapped so the indexed pair form
/// `Object.entries(obj)[0]` also matches.
fn expr_is_entries_call(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => matches!(
            &call.callee,
            Expression::StaticMemberExpression(m) if m.property.name.as_str() == "entries"
        ),
        Expression::ComputedMemberExpression(m) => expr_is_entries_call(&m.object),
        Expression::StaticMemberExpression(m) => expr_is_entries_call(&m.object),
        Expression::ParenthesizedExpression(p) => expr_is_entries_call(&p.expression),
        Expression::TSNonNullExpression(e) => expr_is_entries_call(&e.expression),
        _ => false,
    }
}

/// True when the identifier is bound as an element of an array-destructuring
/// pattern (`[key, value]`) whose initializer is a `.entries()` call. `[key,
/// value]` is the canonical, MDN-blessed destructuring for `Object.entries()` /
/// `Map.entries()` / `Array.entries()` pair iteration, so the per-pair `key` and
/// `value` bindings are self-documenting there. Covers both the for-of iterated
/// expression (`for (const [k, v] of Object.entries(obj))`) and the right-hand
/// side of a destructuring assignment (`const [k, v] = Object.entries(obj)[0]`).
/// The walk stops at a function/program boundary so it only inspects the binding's
/// own surroundings.
fn is_in_entries_destructuring<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut in_array_pattern = false;
    for kind in nodes.ancestor_kinds(node.id()) {
        match kind {
            AstKind::ArrayPattern(_) => in_array_pattern = true,
            AstKind::ForOfStatement(stmt) if in_array_pattern => {
                return expr_is_entries_call(&stmt.right);
            }
            AstKind::VariableDeclarator(d) if in_array_pattern => {
                return d.init.as_ref().is_some_and(expr_is_entries_call);
            }
            AstKind::AssignmentExpression(a) if in_array_pattern => {
                return expr_is_entries_call(&a.right);
            }
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => continue,
        }
    }
    false
}

/// True when the identifier is a function/arrow parameter (inside a FormalParameter).
fn is_function_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::FormalParameter(_)) {
            return true;
        }
        if matches!(
            kind,
            AstKind::VariableDeclaration(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_)
        ) {
            break;
        }
    }
    false
}

/// True when the identifier is the loop variable of a `for...of` /
/// `for await...of` / `for...in` statement (`for (const item of items)`). The
/// singular form of the collection name is the idiomatic, self-documenting
/// choice for an iteration variable — the same rationale that exempts
/// `PARAM_ALLOWED_WORDS` for function parameters. The walk stops at a
/// `BlockStatement`/function/program boundary so loop *body* declarations,
/// which reach the `ForOfStatement` only through its body block, are not
/// mistaken for the binding.
fn is_for_of_or_in_binding<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::ForOfStatement(_) | AstKind::ForInStatement(_)) {
            return true;
        }
        if matches!(
            kind,
            AstKind::BlockStatement(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_)
        ) {
            break;
        }
    }
    false
}

/// True when the identifier sits inside an `import { … }` / `import x from …`
/// / `import * as x from …` declaration. The author has no rename freedom
/// for a third-party export (e.g. `import { Result } from "better-result"`).
fn is_in_import_declaration<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::ImportDeclaration(_)) {
            return true;
        }
    }
    false
}

/// True when the identifier is the *name* of a TypeScript generic type
/// parameter declaration (`function f<Value>()`, `class C<Data>`,
/// `type T<Item> = …`). Names like `<Value>`/`<Result>` are the idiomatic,
/// self-documenting equivalent of `<T>` — a type-level placeholder, not a
/// value carrying domain meaning, so they are out of this rule's scope.
fn is_type_parameter_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    matches!(nodes.kind(parent_id), AstKind::TSTypeParameter(_))
}

/// Look at the surrounding FormalParameter / VariableDeclarator's type
/// annotation and decide whether it carries enough domain information to
/// justify a generic name. We accept three shapes:
///
/// - A type reference named `Result` (e.g. `result: Result<User, Err>`).
/// - A type reference named `Promise<Result<…>>` (the awaited form).
/// - An array type (`readonly TRow[]`, `User[]`, `Array<User>`) — for
///   helpers like `firstOrError<TRow>(rows: readonly TRow[], …)`.
/// - A type assertion on the initializer (`const rows = expr as T[]`) —
///   covers generic DB helpers where the cast carries the type information.
fn type_annotation_is_descriptive<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node.id()) {
        match kind {
            AstKind::FormalParameter(p) => {
                let Some(ann) = p.type_annotation.as_ref() else {
                    return false;
                };
                return ts_type_is_descriptive(&ann.type_annotation);
            }
            AstKind::VariableDeclarator(d) => {
                if let Some(ann) = d.type_annotation.as_ref() {
                    return ts_type_is_descriptive(&ann.type_annotation);
                }
                // No explicit annotation: accept a type assertion on the
                // initializer (`const rows = expr as InferSelectModel<TRef>[]`).
                return init_has_descriptive_type_assertion(d.init.as_ref());
            }
            // Stop walking when we leave the binding's surroundings.
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => continue,
        }
    }
    false
}

/// True when `init` is a `TSAsExpression` (or parenthesized wrapper) whose
/// asserted type passes `ts_type_is_descriptive`. Handles:
///   `expr as T[]`  →  TSAsExpression { type_annotation: TSArrayType }
fn init_has_descriptive_type_assertion(init: Option<&Expression>) -> bool {
    let Some(expr) = init else { return false };
    match expr {
        Expression::TSAsExpression(as_expr) => ts_type_is_descriptive(&as_expr.type_annotation),
        Expression::ParenthesizedExpression(paren) => {
            init_has_descriptive_type_assertion(Some(&paren.expression))
        }
        _ => false,
    }
}

/// True when the binding's explicit type annotation is exactly `unknown`. An
/// `unknown`-typed intermediate is a deliberate, irreducible type statement —
/// e.g. bridging an `any`-returning call (`Reflect.apply`) to satisfy
/// `no-unsafe-return` — for which a generic name like `result` is appropriate:
/// the value is opaque by design and a more specific name would assert a
/// semantic identity the code cannot make. (Closes #601)
fn binding_annotation_is_unknown<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let is_unknown = |ann: &Option<oxc_allocator::Box<'_, TSTypeAnnotation<'_>>>| {
        ann.as_ref()
            .is_some_and(|a| matches!(a.type_annotation, TSType::TSUnknownKeyword(_)))
    };
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        match kind {
            AstKind::FormalParameter(p) => return is_unknown(&p.type_annotation),
            AstKind::VariableDeclarator(d) => return is_unknown(&d.type_annotation),
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => continue,
        }
    }
    false
}

fn ts_type_is_descriptive(ty: &TSType) -> bool {
    match ty {
        // `Result<...>`, `Promise<Result<...>>`, etc.
        TSType::TSTypeReference(type_ref) => {
            let name = match &type_ref.type_name {
                TSTypeName::IdentifierReference(id) => Some(id.name.as_str()),
                TSTypeName::QualifiedName(q) => Some(q.right.name.as_str()),
                _ => None,
            };
            if matches!(name, Some("Result")) {
                return true;
            }
            // TypeScript generic type parameter convention: single capital
            // letter (T, R, E, K, V) or T followed by uppercase (TData,
            // TResult, TVariables). These carry type information via the
            // caller's generic instantiation.
            if let Some(n) = name {
                let b = n.as_bytes();
                if (b.len() == 1 && b[0].is_ascii_uppercase())
                    || (b.len() >= 2 && b[0] == b'T' && b[1].is_ascii_uppercase())
                {
                    return true;
                }
            }
            // `Promise<Result<...>>` / `Readonly<Foo[]>` / `Array<T>` —
            // recurse into the type argument.
            if matches!(name, Some("Promise") | Some("Readonly") | Some("Array") | Some("ReadonlyArray")) {
                if let Some(params) = &type_ref.type_arguments
                    && let Some(first) = params.params.first()
                {
                    return ts_type_is_descriptive(first);
                }
            }
            false
        }
        // `T[]` / `readonly T[]`.
        TSType::TSArrayType(_) => true,
        // `readonly T[]` shows up as TSTypeOperator (op = Readonly) wrapping
        // a TSArrayType in oxc's AST.
        TSType::TSTypeOperatorType(op) => ts_type_is_descriptive(&op.type_annotation),
        // `Result | null`, `Result | undefined`.
        TSType::TSUnionType(u) => u.types.iter().any(ts_type_is_descriptive),
        _ => false,
    }
}

/// True when the identifier is a function parameter inside an arrow function
/// or function expression that is used as an object property value.
/// Covers TanStack Query callbacks like `useMutation({ onSuccess: (data) => {} })`
/// where the library's own API prescribes the parameter name.
fn is_in_callback_property<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut func_node_id = None;
    for (kind, nid) in nodes.ancestor_kinds(node.id()).zip(nodes.ancestor_ids(node.id())) {
        match kind {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                func_node_id = Some(nid);
                break;
            }
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => continue,
            _ => break,
        }
    }
    let Some(fid) = func_node_id else { return false };
    let parent_id = nodes.parent_id(fid);
    if parent_id == fid {
        return false;
    }
    matches!(nodes.kind(parent_id), AstKind::ObjectProperty(_))
}

/// Property names that, when assigned a function value, mark that function as a
/// message-event handler (`self.onmessage = …`, `worker.onmessageerror = …`).
const MESSAGE_HANDLER_PROPERTIES: &[&str] = &["onmessage", "onmessageerror"];

/// Event-type string literals that, as the first `addEventListener` argument,
/// mark the listener as a message-event handler.
const MESSAGE_EVENT_TYPES: &[&str] = &["message", "messageerror"];

/// True when the enclosing function of `node` is a message-event handler, i.e.
/// either assigned to an `onmessage`/`onmessageerror` property
/// (`self.onmessage = (data) => …`, `worker.onmessage = function (data) {}`) or
/// passed as the listener argument of `addEventListener('message', …)` /
/// `addEventListener('messageerror', …)`. The handler's first parameter is a
/// `MessageEvent`, so naming it `data` mirrors the platform `MessageEvent.data`
/// contract. The walk stops at the first enclosing arrow/function expression so
/// only that function's own definition site is inspected.
fn is_message_handler_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut func_node_id = None;
    for (kind, nid) in nodes.ancestor_kinds(node.id()).zip(nodes.ancestor_ids(node.id())) {
        match kind {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                func_node_id = Some(nid);
                break;
            }
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => continue,
            _ => break,
        }
    }
    let Some(fid) = func_node_id else { return false };
    let parent_id = nodes.parent_id(fid);
    if parent_id == fid {
        return false;
    }
    match nodes.kind(parent_id) {
        // `self.onmessage = (data) => …` / `worker.onmessage = function (data) {}`
        AstKind::AssignmentExpression(assign) => {
            let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
                return false;
            };
            MESSAGE_HANDLER_PROPERTIES.contains(&member.property.name.as_str())
        }
        // `addEventListener('message', (data) => …)` or
        // `target.addEventListener('message', (data) => …)`. The function is, by
        // construction, an argument of this call (it is its AST parent), and the
        // first argument being the message-event string literal places the
        // listener at a later index — so no per-argument span check is needed.
        AstKind::CallExpression(call) => {
            let callee_name = match &call.callee {
                Expression::StaticMemberExpression(m) => m.property.name.as_str(),
                Expression::Identifier(id) => id.name.as_str(),
                _ => return false,
            };
            if callee_name != "addEventListener" {
                return false;
            }
            let Some(Argument::StringLiteral(event_type)) = call.arguments.first() else {
                return false;
            };
            MESSAGE_EVENT_TYPES.contains(&event_type.value.as_str())
        }
        _ => false,
    }
}

/// True when the identifier is a property key in an object literal.
fn is_object_literal_key<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent_kind = nodes.kind(parent_id);
    if let AstKind::ObjectProperty(prop) = parent_kind {
        // Check if we're the key, not the value
        let key_span = match &prop.key {
            PropertyKey::StaticIdentifier(id) => Some(id.span),
            _ => None,
        };
        if let Some(ks) = key_span {
            let node_span = node.kind().span();
            return ks.start == node_span.start && ks.end == node_span.end;
        }
    }
    false
}

/// True when the identifier is a method call property (`obj.execute()`).
fn is_method_call_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent_kind = nodes.kind(parent_id);
    if let AstKind::StaticMemberExpression(member) = parent_kind {
        // We must be the property
        let node_span = node.kind().span();
        if member.property.span.start == node_span.start {
            // And the member must be called
            let gp_id = nodes.parent_id(parent_id);
            if gp_id != parent_id {
                return matches!(nodes.kind(gp_id), AstKind::CallExpression(_));
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IdentifierReference, AstType::BindingIdentifier]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir || ctx.file.path_segments.in_storybook {
            return;
        }

        let (name, span) = match node.kind() {
            AstKind::BindingIdentifier(id) => (id.name.as_str(), id.span),
            AstKind::IdentifierReference(id) => (id.name.as_str(), id.span),
            _ => return,
        };

        // Identifiers introduced by `import { Foo } from "..."` cannot
        // be renamed by the author — third-party exports are out of their
        // control. Same for default / namespace imports.
        if is_in_import_declaration(node, semantic) {
            return;
        }

        // TypeScript generic type parameter names (`function f<Value>()`,
        // `type T<Item> = …`) are type-level placeholders equivalent to `<T>`,
        // not value identifiers carrying domain meaning.
        if is_type_parameter_name(node, semantic) {
            return;
        }

        // Check banned words — only at declaration sites (BindingIdentifier)
        if let AstKind::BindingIdentifier(_) = node.kind()
            && !is_destructuring(node, semantic)
            {
                let lower = name.to_ascii_lowercase();
                if BANNED_WORDS.contains(&lower.as_str()) {
                    if PARAM_ALLOWED_WORDS.contains(&lower.as_str())
                        && (is_function_param(node, semantic)
                            || is_for_of_or_in_binding(node, semantic))
                    {
                        return;
                    }
                    // `[key, value]` destructured from a `.entries()` call is the
                    // canonical pair-iteration idiom (Object/Map/Array.entries);
                    // the per-pair `key`/`value` bindings are self-documenting.
                    if is_in_entries_destructuring(node, semantic) {
                        return;
                    }
                    // A descriptive type annotation (`result: Result<…>`,
                    // `rows: readonly TRow[]`) carries the domain info the
                    // identifier name would otherwise need to.
                    if type_annotation_is_descriptive(node, semantic) {
                        return;
                    }
                    // An explicit `unknown` annotation is a deliberate opacity
                    // statement (bridging an `any`-returning call to satisfy
                    // no-unsafe-return); a generic name fits an opaque value.
                    if binding_annotation_is_unknown(node, semantic) {
                        return;
                    }
                    // The identifier is a parameter of an arrow/function that is
                    // an object-literal property value (e.g. TanStack Table's
                    // `cell: (info) => …`, `accessorFn: (row) => …`). The
                    // library's own API prescribes the parameter name, so the
                    // author has no rename freedom — same rationale already
                    // applied to banned prefixes (`onSuccess: (data) => …`).
                    if is_in_callback_property(node, semantic) {
                        return;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Identifier '{name}' carries no meaning — rename to describe \
                             what the value IS (`parsedOrder`, `userProfile`, \
                             `paymentReceipt`)."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }

        // Check banned prefixes — only at declaration sites (BindingIdentifier).
        // Checking references would re-flag every usage of a destructured name
        // like `data` from `const { data } = useQuery(...)`.
        if !matches!(node.kind(), AstKind::BindingIdentifier(_)) {
            return;
        }
        if is_destructuring(node, semantic) {
            return;
        }
        if is_object_literal_key(node, semantic) {
            return;
        }
        if is_method_call_name(node, semantic) {
            return;
        }
        if GLOBAL_IDENTIFIER_ALLOWLIST.contains(&name) {
            return;
        }

        if let Some(prefix) = matched_banned_prefix(name) {
            // An identifier mirroring a standard DOM/Web API type name in
            // camelCase (`dataTransfer` for `DataTransfer`) refers to the web
            // platform object, not a generic `data*` compound.
            if mirrors_dom_api_name(name) {
                return;
            }
            if prefix == "data" && DATA_PASCAL_CASE_ALLOWED_COMPOUNDS.iter().any(|allowed| match name.strip_prefix(allowed) {
                Some("") => true,
                Some(rest) => rest.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase()),
                None => false,
            }) {
                return;
            }
            // A class extending a Grafana datasource SDK base
            // (`class DataSource extends DataSourceApi<…>`) is a
            // framework-mandated plugin entry name, not a lazy `data*` compound.
            if prefix == "data" && is_grafana_datasource_class_name(node, ctx, semantic) {
                return;
            }
            // A descriptive type annotation (e.g. `data: TData`) carries
            // the domain information the identifier name would otherwise need.
            if type_annotation_is_descriptive(node, semantic) {
                return;
            }
            // The function is a property value in an object literal
            // (e.g. TanStack Query's `onSuccess: (data) => {}`). The
            // library's own API prescribes `data` as the parameter name.
            if is_in_callback_property(node, semantic) {
                return;
            }
            // The `data` parameter of a message-event handler (`self.onmessage =
            // (data) => …`, `addEventListener('message', (data) => …)`) mirrors
            // the platform `MessageEvent.data` contract — the idiomatic name.
            if prefix == "data"
                && is_function_param(node, semantic)
                && is_message_handler_param(node, semantic)
            {
                return;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Identifier '{name}' uses banned prefix '{prefix}' — use \
                     intent over implementation. Try: what does this actually \
                     accomplish? (`processOrder` → `fulfillOrder`, `doPayment` → \
                     `chargeCustomer`)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_bare_result_local_const() {
        let src = r#"function f() { const result = 1; return result; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_result_imported_from_better_result() {
        // Regression for rbaumier/comply#39 case 1 — third-party imports.
        let src = r#"import { Result } from "better-result"; const x: Result<number, Error> = anything;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_result_default_import() {
        // Regression for #214 — default imports.
        let src = r#"import Result from "better-result";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_result_namespace_import() {
        // Regression for #214 — namespace imports.
        let src = r#"import * as Result from "better-result";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_renamed_named_import_to_result() {
        // Regression for #214 — renamed local binding.
        let src = r#"import { Foo as Result } from "lib";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_user_declared_result_const() {
        // Negative: user-declared, not imported — must still flag.
        let src = r#"const Result = somethingElse;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_user_declared_result_function() {
        // Negative: user-declared function — must still flag.
        let src = r#"function Result() { return 1; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_result_when_typed_as_Result() {
        // Regression for rbaumier/comply#39 case 2 — canonical Result name.
        let src = r#"
            async function unwrapOrThrow<T, E>(p: Promise<Result<T, E>>): Promise<T> {
                const result: Result<T, E> = await p;
                if (result.isErr()) { throw result.error; }
                return result.value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_rows_parameter_typed_as_array() {
        // Regression for rbaumier/comply#39 case 3 — generic-helper rows.
        let src = r#"
            function firstOrError<TRow>(rows: readonly TRow[], message: string): TRow {
                return rows[0];
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_untyped_rows_param() {
        let src = r#"function f(rows) { return rows[0]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_data_table_design_system_compound() {
        // Regression for rbaumier/comply#121 — `DataTable` and its derived
        // type names are the industry-standard naming across shadcn,
        // TanStack Table, Material UI, and Radix.
        let src = r#"
            export function DataTable() { return null; }
            export type DataTableSort = { field: string };
            export type DataTableState = { sort: DataTableSort };
            export type DataGridColumn = { key: string };
            export type DataViewProps = { rows: number };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_data_prefix_on_non_canonical_compounds() {
        // `DataSource`, `DataObject`, `DataValue` carry no more meaning than
        // `data` alone — the `Data` allowlist is intentionally narrow to the
        // canonical UI primitives.
        let src = r#"const dataSource = 1; const DataObject = {}; const DataValue = 2;"#;
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn no_fp_grafana_datasource_class_extends_sdk_base_issue_1531() {
        // Regression for #1531 — the Grafana plugin SDK mandates the datasource
        // implementation class be named `DataSource` and extend `DataSourceApi`
        // (or `DataSourceWithBackend`). The structural `extends` is what proves
        // it is the framework-prescribed entry point, not a lazy `data*` name.
        let src = r#"
            export class DataSource extends DataSourceApi<MyQuery, MyDataSourceOptions> {
                query() { return null; }
            }
            export class BackendDataSource extends DataSourceWithBackend<MyQuery, MyOptions> {}
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_bare_data_source_class_without_grafana_base_issue_1531() {
        // Negative space: a bare `class DataSource {}` with no Grafana base is a
        // genuinely generic name and must still fire. The exemption is purely
        // structural — it hinges on the `extends DataSourceApi` heritage, not on
        // the literal name `DataSource` or its directory.
        let src = r#"
            class DataSource {}
            class Manager {}
            class DataSourceExtendingOther extends SomethingElse {}
        "#;
        // `DataSource` and `DataSourceExtendingOther` use the `data` prefix;
        // `Manager` is not generic, so exactly two fire.
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_data_last_data_first_fp_compounds() {
        // Regression for rbaumier/comply#973 — `DataLast`/`DataFirst` are the
        // fp-ts/Effect terms for the `dual()` calling conventions, not generic
        // `data` names.
        let src = r#"
            function dual<DataLast extends (...args: any[]) => any, DataFirst extends (...args: any[]) => any>(body: DataFirst): DataLast & DataFirst { return body as any; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lookup_function_and_variable() {
        // Regression for rbaumier/comply#973 — `lookup` is the canonical FP
        // term for "get by key/index" (Haskell `Data.Map.lookup`).
        let src = r#"
            export const lookup = (arr: number[], i: number) => arr[i];
            function f() { const lookup = {}; return lookup; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_other_banned_words() {
        // Negative: the banned-word list stays intact apart from `lookup`.
        let src = r#"const obj = 1; const temp = 2;"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_data_param_typed_with_generic_type_param() {
        // Regression for #337 — TanStack Query type aliases annotate the
        // `data` parameter with a generic type parameter like `TData`.
        let src = r#"
            type InvalidateOption<TData, TVariables> =
              | ((data: TData, variables: TVariables) => string[])
              | null;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_data_param_in_tanstack_mutation_callback() {
        // Regression for #337 — TanStack Query's onSuccess callback has
        // `data` as its first parameter per the library's own type signature.
        let src = r#"
            useMutation({
                onSuccess: (data, variables) => {
                    console.log(data);
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_data_param_untyped_in_function_declaration() {
        // `data` without a type annotation in a named function declaration
        // is still vague — no library convention prescribes it there.
        let src = r#"function myFunc(data) { return data; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_rows_with_type_assertion_in_generic_db_helper() {
        // Regression for #389 — `rows` cast to a generic array type in a
        // generic database helper where no domain-specific name is possible.
        let src = r#"
            export async function replaceTeamJunction<
                TJunction extends PgTable,
                TRef extends PgTable,
            >(tx: DatabaseTransaction): Promise<void> {
                return Result.gen(async function* () {
                    const rows = (yield* Result.await(
                        tryDatabaseQuery(() => tx.select().from(junctionTable)),
                    )) as InferSelectModel<TRef>[];
                    return Result.ok(rows);
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_rows_without_type_assertion() {
        // No `as T[]` — must still flag.
        let src = r#"
            async function genericHelper<TRef extends PgTable>(tx: any): Promise<void> {
                const rows = await tx.select().from(table);
                return rows;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_run_with_context_async_local_storage_wrapper() {
        // Regression for #520 — `runWith*` is the idiomatic AsyncLocalStorage
        // wrapper pattern; `run` comes from the Node.js API, not a generic verb.
        let src = r#"
            export function runWithRequestContext<T>(context: RequestContext, callback: () => T): T {
                return requestContextStorage.run(context, callback);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_run_with_transaction_async_local_storage_wrapper() {
        // Same pattern for transaction context.
        let src = r#"
            export function runWithTransaction<T>(tx: Transaction, fn: () => T): T {
                return transactionStorage.run(tx, fn);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_result_typed_unknown_bridge_issue_601() {
        // `const result: unknown = Reflect.apply(...)` — the `unknown`
        // annotation is the only way to escape Reflect.apply's `any` return and
        // satisfy no-unsafe-return; the value is opaque by design.
        let src = r#"
            function apply(target: unknown, args: unknown[]): unknown {
                const result: unknown = Reflect.apply(target, null, args);
                return result;
            }
        "#;
        assert!(run(src).is_empty(), "result: unknown bridge should not flag");
    }

    #[test]
    fn still_flags_untyped_result_const_issue_601() {
        // Without the `unknown` annotation, a bare `result` is still vague.
        let src = r#"function f() { const result = compute(); return result; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_domain_noun_with_descriptive_camel_suffix_issue_1383() {
        // Regression for #1383 — `run`/`data` as domain nouns combined with an
        // identifying suffix (`Id`, `Status`, `Type`, `Key`) name a specific
        // concept, not a generic action (trigger.dev's `runId`/`RunStatus`).
        let src = r#"
            const runId = task.runId;
            const runFriendlyId = run.id;
            type RunStatus = "WAITING" | "EXECUTING" | "COMPLETED";
            const dataKey = "x";
            const dataType = "json";
            const dataJson = "{}";
            const runAt = new Date();
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_run_task_generic_verb() {
        // `runTask` uses `run` as a generic verb — must still flag.
        let src = r#"function runTask() {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_generic_verbs_without_descriptive_suffix_issue_1383() {
        // The suffix exemption is narrow: compounds whose tail is not a
        // domain-identifying suffix stay flagged. `dataSource`/`runJob` carry
        // no more meaning than the bare prefix.
        let src = r#"const dataSource = 1; function runJob() {} const processOrder = 2;"#;
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn still_flags_run_migration_generic_verb() {
        // `runMigration` uses `run` as a generic verb — must still flag.
        let src = r#"function runMigration() {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_run_compound_with_specific_multiword_suffix_issue_1394() {
        // Regression for #1394 — a `run`-prefixed compound whose suffix names
        // *what* is run with ≥2 capitalized words is self-documenting.
        let src = r#"
            function runBeforeUnload(frame) { return frame; }
            async function runCommandOnSnapshot(snapshot, command) { return command; }
            async function runNpmAudit() { return 0; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_run_single_word_filler_suffix_issue_1394() {
        // Negative: single-word suffixes stay generic fillers — `runTask`,
        // `runJob`, `runStuff`, `runProcess` carry no more meaning than `run`.
        let src = r#"
            function runTask() {}
            function runJob() {}
            function runStuff() {}
            function runProcess() {}
        "#;
        assert_eq!(run(src).len(), 4);
    }

    #[test]
    fn flags_row_and_rows_in_iterator_callbacks() {
        // Iterator-callback params are not exempt — `row`/`rows` are vague
        // even as `.map`/`.flatMap` parameters (e.g. valuation-tariff readers).
        let src = r#"
            readSheetRows(buffer, { skipRows: 1 }).map((rows) =>
                rows.flatMap((row): Out[] => {
                    return [row[0]];
                }),
            );
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn still_allows_value_and_item_params_in_iterator_callbacks() {
        // `value`/`item` stay allowed as function parameters (PARAM_ALLOWED_WORDS).
        assert!(run("[1].map((value) => value);").is_empty());
        assert!(run("[1].map((item) => item);").is_empty());
    }

    #[test]
    fn no_fp_generic_type_parameter_names_issue_1386() {
        // Regression for #1386 — `<Value>`/`<Result>`/`<Data>`/`<Item>` are
        // idiomatic, self-documenting names for TypeScript generic type
        // parameters (the readable equivalent of `<T>`), not value identifiers.
        let src = r#"
            type Getter = <Value>(atom: Atom<Value>) => Value;
            type Setter = <Value, Args extends unknown[], Result>(
                atom: WritableAtom<Value, Args, Result>,
                ...args: Args
            ) => Result;
            type Read<Value, SetSelf = never> = (get: Getter, setSelf: SetSelf) => Value;
            function identity<Item>(item: Item): Item { return item; }
            class Container<Data> { has(d: Data): boolean { return true; } }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_result_as_value_identifiers() {
        // Negative: `Value`/`Result` declared as variables (not type
        // parameters) carry no meaning and must still flag.
        let src = r#"const Value = 1; const Result = 2;"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_data_transfer_mirrors_dom_api_name_issue_1388() {
        // Regression for #1388 — `dataTransfer` mirrors the DOM `DataTransfer`
        // type name (`DragEvent.dataTransfer`); it refers to the web platform
        // object, not a generic `data*` compound.
        let src = r#"
            type ScheduleProps = {
                onExternalEventDrop?: (dataTransfer: DataTransfer, dropDateTime: string) => void;
            };
            function handleDrop(dataTransfer: DataTransfer) { return dataTransfer; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_generic_data_compound_not_in_dom_allowlist_issue_1388() {
        // Negative: the DOM allowlist is exact-name only. `dataPayload`/
        // `dataResponse` are generic `data*` compounds and must still flag.
        let src = r#"const dataPayload = 1; const dataResponse = 2;"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_data_url_web_standard_concept_issue_1162() {
        // Regression for #1162 — a "data URL" (RFC 2397 `data:` scheme) is a
        // specific web-standard concept, mirroring `FileReader.readAsDataURL`.
        // Both casings (`dataUrl`, `dataURL`) name the platform value, not a
        // generic `data*` compound.
        let src = r#"
            const dataUrl = reader.result as string;
            const dataURL = canvas.toDataURL();
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_bare_data_and_generic_data_value_issue_1162() {
        // Negative: the allowlist is exact-name only — bare `data` and the
        // generic `dataValue` compound carry no domain meaning and still flag.
        let src = r#"const data = 1; const dataValue = 2;"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_item_in_for_await_of_loop_binding_issue_1163() {
        // Regression for #1163 — `item` as a for-await-of loop variable is the
        // idiomatic singular form of the collection, like a function parameter.
        let src = r#"
            const resArray = [];
            for await (let item of client.entities.list()) {
                resArray.push(item);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_item_value_in_for_of_and_for_in_loop_bindings_issue_1163() {
        // Both `for...of` and `for...in` loop bindings exempt PARAM_ALLOWED_WORDS.
        assert!(run("for (const item of items) { use(item); }").is_empty());
        assert!(run("for (const value of values) { use(value); }").is_empty());
        assert!(run("for (const item in obj) { use(item); }").is_empty());
    }

    #[test]
    fn still_flags_item_as_bare_const_not_loop_binding_issue_1163() {
        // Negative: a plain `const item = ...` is neither a parameter nor a loop
        // binding — it must still flag.
        let src = r#"function f() { const item = compute(); return item; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_item_declared_in_for_of_loop_body_issue_1163() {
        // Negative: the exemption is for the loop *binding* only — a generic
        // name declared inside the loop body is still flagged.
        let src = r#"for (const entity of entities) { const item = entity.x; use(item); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_cell_and_cells_banned_words() {
        assert_eq!(run("const cell = 1;").len(), 1);
        assert_eq!(run("const cells = [];").len(), 1);
    }

    #[test]
    fn no_fp_tanstack_table_column_def_callback_params_issue_1716() {
        // Regression for #1716 — TanStack Table column definitions prescribe
        // `row`/`cell`/`info` as the callback parameter names (`Row<TData>`,
        // `CellContext<TData, TValue>`). The params are values of object-literal
        // properties (`cell:`, `accessorFn:`), so the library API fixes the name.
        let src = r#"
            const columns = [
                {
                    accessorKey: 'firstName',
                    cell: (info) => info.getValue(),
                },
                {
                    accessorFn: (row) => row.lastName,
                    cell: (info) => info.getValue(),
                },
                {
                    header: (info) => info.column.id,
                },
            ];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_row_cell_info_params_in_call_argument_callbacks_issue_1716() {
        // Negative space: the exemption is for callbacks that are object-literal
        // *property values*. A `row`/`cell`/`info` param of a callback passed as
        // a *call argument* (`.map((row) => …)`) is not API-prescribed and is
        // still a vague name — must still flag.
        let src = r#"
            items.map((row) => row[0]);
            items.forEach((cell) => use(cell));
            items.filter((info) => info.ok);
        "#;
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn no_fp_key_value_destructured_from_entries_issue_1319() {
        // Regression for #1319 — `[key, value]` is the canonical destructuring
        // for `Object.entries()` / `Map.entries()` / `Array.entries()` pair
        // iteration; renaming hurts readability. Covers the for-of iterated
        // expression and the indexed destructuring-assignment form.
        let src = r#"
            for (const [key, value] of Object.entries(obj)) { use(key, value); }
            for (const [name, value] of Object.entries(env)) { use(name, value); }
            for (const [key, value] of m.entries()) { use(key, value); }
            for (const [index, value] of arr.entries()) { use(index, value); }
            const [k, value] = Object.entries(params)[0];
            const [k2, value] = arr.entries()[i];
        "#;
        assert!(run(src).is_empty(), "entries destructuring must not flag");
    }

    #[test]
    fn still_flags_value_destructured_from_non_entries_call_issue_1319() {
        // Negative space: the exemption is scoped to `.entries()`. A `value`
        // bound from an array-destructuring whose initializer is some other call
        // is not the entries idiom and must still flag.
        let src = r#"const [first, value] = getPair();"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_bare_value_const_and_data_param_issue_1319() {
        // Negative space: plain generic names outside the entries context keep
        // firing — a bare `const value = ...` and a `data` parameter.
        let src = r#"
            function g() { const value = getThing(); return value; }
            function f(data) { return data; }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_data_param_in_message_event_handler_issue_1499() {
        // Regression for #1499 — the `data` parameter of a Web Worker /
        // postMessage message-event handler mirrors the platform
        // `MessageEvent.data` contract, so it is the idiomatic name there.
        let src = r#"
            self.onmessage = (data) => { use(data); };
            worker.onmessage = function (data) { use(data); };
            self.onmessageerror = (data) => { use(data); };
            addEventListener('message', (data) => { use(data); });
            target.addEventListener('messageerror', function (data) { use(data); });
        "#;
        assert!(run(src).is_empty(), "message-handler `data` param must not flag");
    }

    #[test]
    fn still_flags_data_param_in_non_message_contexts_issue_1499() {
        // Negative space: the exemption is scoped to the message-handler context.
        // A plain function param and a non-`message` event listener keep firing.
        let src = r#"
            function process(data) { return data; }
            element.addEventListener('click', (data) => { use(data); });
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn does_not_flag_row_cell_as_prefix_or_suffix() {
        // row/rows/cell/cells are exact-name bans only — compounds like
        // `rowIndex` or `headerCell` carry meaning and must not be flagged.
        for name in [
            "tableRow",
            "rowIndex",
            "firstRow",
            "rowCount",
            "headerCell",
            "cellValue",
            "cellRenderer",
        ] {
            let src = format!("const {name} = 1;");
            assert!(run(&src).is_empty(), "'{name}' must NOT be flagged");
        }
    }
}
