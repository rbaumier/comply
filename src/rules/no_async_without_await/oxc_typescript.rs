//! no-async-without-await OXC backend — flag `async` functions that contain
//! no `await` or `for await` in their own body.

use rustc_hash::{FxHashMap, FxHashSet};
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{ClassShape, byte_offset_to_line_col, enclosing_class};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

/// Check if a function node has an explicit Promise return type annotation.
fn has_promise_return_type(
    source: &str,
    return_type: &Option<oxc_allocator::Box<oxc_ast::ast::TSTypeAnnotation>>,
) -> bool {
    let Some(rt) = return_type else { return false };
    let text = &source[rt.span.start as usize..rt.span.end as usize];
    text.contains("Promise<") || text.contains("PromiseLike<")
}

/// Check if an async arrow is the initializer of a variable whose explicit type
/// annotation declares a `Promise`-returning function type, e.g.
/// `const readAsset: (id: string) => Promise<Buffer> = async () => {...}`. The
/// binding annotation owns the contract: without `async`, the arrow's inferred
/// return type (e.g. `never` for a sync-throw body) would not satisfy the
/// declared `Promise<T>`, so `async` is mandatory even when the body never
/// awaits. Mirrors the arrow's own `has_promise_return_type` exemption, reading
/// the annotation off the `VariableDeclarator` binding instead of the arrow.
fn is_arrow_bound_to_promise_annotation(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    if !matches!(func_node.kind(), AstKind::ArrowFunctionExpression(_)) {
        return false;
    }
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().parent_node(func_node.id()).kind()
    else {
        return false;
    };
    has_promise_return_type(source, &decl.type_annotation)
}

/// Find the nearest enclosing async function/arrow for a given node,
/// stopping at function boundaries. Returns the NodeId of the nearest
/// enclosing function/arrow.
fn nearest_function_id(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return Some(ancestor.id());
            }
            _ => {}
        }
    }
    None
}

/// Check if the function/arrow node is passed as an argument to a call
/// expression (i.e. it is a callback). In oxc's semantic tree, arguments have
/// no wrapper node, so a callback's immediate parent is the `CallExpression`
/// itself. The callee position (an IIFE like `(async () => {})()`) is excluded
/// by requiring the node to appear in the call's `arguments`.
fn is_call_argument(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(func_node.id());
    let AstKind::CallExpression(call) = parent.kind() else { return false };
    let span = func_node.kind().span();
    call.arguments
        .iter()
        .any(|arg| arg.span() == span)
}

/// Check if the function/arrow node is the value of a JSX attribute, e.g.
/// `<form action={async () => {}}>`. The parent chain is `Function ->
/// JSXExpressionContainer -> JSXAttribute`. Like a bare call-argument callback,
/// the attribute's type contract owns the signature: JSX props such as the
/// Next.js App Router `<form action>` are typed `() => Promise<void>`, so `async`
/// is mandatory even when the body fires a bound action without awaiting it. The
/// author does not control the call site, so the missing `await` is not a smell.
fn is_jsx_attribute_value(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let container = nodes.parent_node(func_node.id());
    let AstKind::JSXExpressionContainer(_) = container.kind() else {
        return false;
    };
    matches!(
        nodes.parent_node(container.id()).kind(),
        AstKind::JSXAttribute(_)
    )
}

/// Check if the async function is the value of an object-literal property whose
/// object is — directly or through one or more nested object levels — an argument
/// of a call expression. Covers the shorthand-method shape
/// (`$config({ async run() {} })`), the arrow-value shape
/// (`useForm({ onSubmit: async () => {} })`), and methods nested in sub-objects of
/// the call argument (`defineStore({ actions: { async fn() {} } })`). The walk
/// climbs consecutive `ObjectProperty -> ObjectExpression` hops, succeeding as
/// soon as an `ObjectExpression` is an argument of a `CallExpression`. Like a bare
/// arrow callback, the callee owns the contract: framework/library options objects
/// type such callbacks `(...) => Promise<T>` (SST/Pulumi `run()`, TanStack Form
/// `onSubmit`, Pinia `defineStore` actions), so `async` is mandatory even when the
/// body never awaits. Any node interrupting the pure ObjectProperty/ObjectExpression
/// chain before the call argument (a plain object literal, a class member, ...)
/// breaks the walk and keeps the function flagged.
fn is_object_property_in_call_arg(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    let mut property = nodes.parent_node(func_node.id());
    loop {
        let AstKind::ObjectProperty(_) = property.kind() else { return false };

        let object = nodes.parent_node(property.id());
        let AstKind::ObjectExpression(_) = object.kind() else { return false };

        let parent = nodes.parent_node(object.id());
        if let AstKind::CallExpression(call_expr) = parent.kind() {
            let object_span = object.kind().span();
            return call_expr
                .arguments
                .iter()
                .any(|arg| arg.span() == object_span);
        }

        // The object is the value of an outer property: climb another hop.
        property = parent;
    }
}

/// Check if a function/arrow body is exactly one `return <CallExpression>;`,
/// i.e. it forwards another call's result. Such a function delegates its
/// `Promise` return to the callee; `async` declares the `Promise` return type
/// (mirroring the companion `promise-function-async` rule) and dropping it would
/// break the type contract, so the absent `await` is not a smell. This is the
/// block-body analog of the already-exempt concise arrow `async () => call()`.
fn body_is_single_return_call(body: &oxc_ast::ast::FunctionBody) -> bool {
    let [oxc_ast::ast::Statement::ReturnStatement(ret)] = body.statements.as_slice() else {
        return false;
    };
    matches!(
        ret.argument,
        Some(oxc_ast::ast::Expression::CallExpression(_))
    )
}

/// Check if a method node or its class has decorators.
fn has_decorators(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(func_node.id()) {
        if let AstKind::MethodDefinition(method) = ancestor.kind() {
            if !method.decorators.is_empty() {
                return true;
            }
            // Check class decorators.
            for class_ancestor in semantic.nodes().ancestors(ancestor.id()) {
                if let AstKind::Class(class) = class_ancestor.kind() {
                    if !class.decorators.is_empty() {
                        return true;
                    }
                    break;
                }
            }
            return false;
        }
    }
    false
}

/// Check if the async function is a direct member of a class that declares an
/// `implements` clause. The immediate parent must be a `MethodDefinition`
/// (`async formData() {}`) or a `PropertyDefinition` (`handleError = async () =>
/// {}`); a nested arrow inside a method body is not a class member and is not
/// covered. comply is syntactic and cannot read the implemented interface, but
/// `async` on a member of an `implements`-ing class is the standard way to
/// satisfy a Promise-returning interface method without writing the explicit
/// return annotation, so the missing `await` is not a smell.
fn is_method_of_implementing_class(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let member = nodes.parent_node(func_node.id());
    if !matches!(
        member.kind(),
        AstKind::MethodDefinition(_) | AstKind::PropertyDefinition(_)
    ) {
        return false;
    }
    enclosing_class(member.id(), nodes).is_some_and(|class| ClassShape::of(class).has_implements)
}

/// Check if the async function is a class method marked `override`. The
/// `override` keyword binds the method's signature to the parent class's
/// contract (possibly an external type, e.g. the Cloudflare Workers
/// `DurableObject`): when the parent declares the method `async`/`Promise`-
/// returning, the override must stay `async` to preserve the `Promise` return
/// type under Liskov substitution, so the missing `await` is not a smell. The
/// immediate parent must be the `MethodDefinition`; a nested arrow inside the
/// body is not the override and stays flagged. Mirrors the interface-driven
/// `is_method_of_implementing_class` exemption for the `extends`-only case,
/// where `ClassShape::has_implements` is false.
fn is_override_method(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    matches!(
        semantic.nodes().parent_node(func_node.id()).kind(),
        AstKind::MethodDefinition(method) if method.r#override
    )
}

/// Check if the async function is the value of an assignment `this.X = async
/// () => {...}` whose enclosing class declares an `async X()` method. Reassigning
/// the method on the instance must preserve its `Promise`-returning type: callers
/// do `await instance.X()`, so the replacement has to stay `async` to satisfy the
/// declared method's contract — dropping it would change the return type from
/// `Promise<void>` to `void` and break every `await` call site. Same
/// external-contract class as the call-argument and object-property exemptions.
///
/// The matching `async` method declared in the same class body is the
/// load-bearing discriminator: a non-`this` target (`obj.X = async () => {}`), a
/// missing `async X()` declaration, a same-named non-async method, or a method
/// inherited from a superclass all keep the diagnostic firing. Computed targets
/// (`this[x] = ...`) are not covered.
fn is_async_method_override_on_this(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{AssignmentTarget, ClassElement, Expression, PropertyKey};

    let nodes = semantic.nodes();
    let parent = nodes.parent_node(func_node.id());
    let AstKind::AssignmentExpression(assign) = parent.kind() else {
        return false;
    };
    // The function must be the assigned value (RHS), not a sub-expression of the
    // target.
    if assign.right.span() != func_node.kind().span() {
        return false;
    }
    // LHS must be `this.<name>` — a static member rooted directly at `this`.
    let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
        return false;
    };
    if !matches!(member.object, Expression::ThisExpression(_)) {
        return false;
    }
    let property_name = member.property.name.as_str();

    // The enclosing class must declare an `async` method with the same name.
    let Some(class) = enclosing_class(parent.id(), nodes) else {
        return false;
    };
    class.body.body.iter().any(|element| {
        let ClassElement::MethodDefinition(method) = element else {
            return false;
        };
        if !method.value.r#async {
            return false;
        }
        let key_name = match &method.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return false,
        };
        key_name == property_name
    })
}

/// True if `b` can appear inside a JS/TS identifier, used to require that an
/// alias name occurs in a type's text as a standalone identifier rather than as
/// a substring of a longer one (so `Unregister` is not matched inside
/// `NamespacedUnregister`).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// True if `name` occurs in `haystack` bounded by non-identifier characters,
/// i.e. as a standalone identifier reference.
fn mentions_identifier(haystack: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = haystack[search_from..].find(name) {
        let start = search_from + rel;
        let end = start + name.len();
        let before_is_ident = start > 0 && is_ident_byte(bytes[start - 1]);
        let after_is_ident = end < bytes.len() && is_ident_byte(bytes[end]);
        if !before_is_ident && !after_is_ident {
            return true;
        }
        // Advance past this match. `end` is a char boundary (`name` is a
        // substring found at `start`), and since alias names are identifiers,
        // any occurrence overlapping `[start, end)` is preceded by an identifier
        // byte and therefore never standalone — so skipping to `end` is safe.
        search_from = end;
    }
    false
}

/// Collect every `type X = ...` alias declared in the file, mapping the alias
/// name to its right-hand-side source text. Used to resolve a named asserted
/// type (`as NamespacedUnregister`) to the `Promise` it ultimately denotes.
fn build_alias_map<'s>(
    semantic: &oxc_semantic::Semantic,
    source: &'s str,
) -> FxHashMap<&'s str, &'s str> {
    let mut map = FxHashMap::default();
    for node in semantic.nodes().iter() {
        if let AstKind::TSTypeAliasDeclaration(alias) = node.kind() {
            let name = &source[alias.id.span.start as usize..alias.id.span.end as usize];
            let rhs_span = alias.type_annotation.span();
            let rhs = &source[rhs_span.start as usize..rhs_span.end as usize];
            map.insert(name, rhs);
        }
    }
    map
}

/// Detect whether `type_text` denotes a `Promise`-returning type — either
/// directly (the text contains `Promise<`/`PromiseLike<`, mirroring
/// `has_promise_return_type`) or transitively, by following references to type
/// aliases declared in the same file. `aliases` maps each in-file `type X = ...`
/// name to its right-hand-side text; `visited` guards against cyclic aliases.
fn type_text_denotes_promise<'s>(
    type_text: &str,
    aliases: &FxHashMap<&'s str, &'s str>,
    visited: &mut FxHashSet<&'s str>,
) -> bool {
    if type_text.contains("Promise<") || type_text.contains("PromiseLike<") {
        return true;
    }
    for (&name, &rhs) in aliases {
        if visited.contains(name) || !mentions_identifier(type_text, name) {
            continue;
        }
        visited.insert(name);
        if type_text_denotes_promise(rhs, aliases, visited) {
            return true;
        }
    }
    false
}

/// If `func_node` (optionally wrapped in parentheses) is the operand of a
/// `TSAsExpression` or `TSSatisfiesExpression`, return the asserted/satisfied
/// type's source text; otherwise `None`.
fn asserted_type_text<'s>(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &'s str,
) -> Option<&'s str> {
    let nodes = semantic.nodes();
    let mut parent = nodes.parent_node(func_node.id());
    while matches!(parent.kind(), AstKind::ParenthesizedExpression(_)) {
        parent = nodes.parent_node(parent.id());
    }
    let span = match parent.kind() {
        AstKind::TSAsExpression(e) => e.type_annotation.span(),
        AstKind::TSSatisfiesExpression(e) => e.type_annotation.span(),
        _ => return None,
    };
    Some(&source[span.start as usize..span.end as usize])
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return Vec::new();
        }

        // Collect node IDs of functions/arrows that contain an await or for-await.
        let mut has_await: FxHashSet<oxc_semantic::NodeId> =
            FxHashSet::default();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::AwaitExpression(_) => {
                    if let Some(func_id) = nearest_function_id(node, semantic) {
                        has_await.insert(func_id);
                    }
                }
                AstKind::ForOfStatement(for_of) if for_of.r#await => {
                    if let Some(func_id) = nearest_function_id(node, semantic) {
                        has_await.insert(func_id);
                    }
                }
                _ => {}
            }
        }

        // Now check all async functions/arrows.
        let mut diagnostics = Vec::new();

        // Lazily built map of in-file `type X = ...` aliases, used only to
        // resolve a named asserted type (`as NamespacedUnregister`) to the
        // `Promise` it denotes. Built at most once per file, on first need.
        let mut alias_map: Option<FxHashMap<&str, &str>> = None;

        for node in semantic.nodes().iter() {
            let (is_async, return_type, span, has_body) = match node.kind() {
                AstKind::Function(f) => {
                    // An async generator (`async function*`, incl. `async *m()`
                    // methods) needs `async` regardless of any `await`: it sets
                    // the return type to `AsyncIterable<T>` (vs `Iterable<T>` for
                    // a sync `function*`), so the keyword is load-bearing.
                    if f.generator {
                        continue;
                    }
                    (f.r#async, &f.return_type, f.span, f.body.is_some())
                }
                AstKind::ArrowFunctionExpression(f) => {
                    (f.r#async, &f.return_type, f.span, true)
                }
                _ => continue,
            };

            if !is_async || !has_body {
                continue;
            }

            if has_promise_return_type(ctx.source, return_type) {
                continue;
            }

            // Async arrow bound to a variable whose explicit type annotation
            // declares a Promise-returning function type, e.g. `const readAsset:
            // (id: string) => Promise<Buffer> = async () => {...}`. The binding
            // annotation owns the contract: without `async`, the arrow's inferred
            // return type (e.g. `never` for a sync-throw body) would not satisfy
            // the declared `Promise<T>`, so `async` is mandatory even when the
            // body never awaits. This is the binding-annotation analog of the
            // arrow's own `has_promise_return_type` exemption. A non-Promise
            // binding annotation (`() => number`) or no annotation stays flagged.
            if is_arrow_bound_to_promise_annotation(node, semantic, ctx.source) {
                continue;
            }

            // Async arrow/function asserted to a Promise-returning type via a
            // `TSAsExpression`/`TSSatisfiesExpression`, e.g. `(async () => {...})
            // as () => Promise<void>` or `... as NamespacedUnregister` where the
            // named type resolves — through type aliases declared in the same
            // file — to a `Promise`-returning signature. The assertion owns the
            // contract: without `async`, the arrow's inferred return type
            // (`void`/`never`) is not assignable to the asserted `Promise<T>`,
            // so `async` is mandatory even when the body never awaits. Mirrors
            // the `has_promise_return_type` text scan, applied to the asserted
            // type (resolved one or more hops through in-file aliases). A
            // non-Promise asserted type, or a named alias whose definition is
            // not in this file, stays flagged.
            if let Some(asserted) = asserted_type_text(node, semantic, ctx.source) {
                let aliases =
                    alias_map.get_or_insert_with(|| build_alias_map(semantic, ctx.source));
                let mut visited = FxHashSet::default();
                if type_text_denotes_promise(asserted, aliases, &mut visited) {
                    continue;
                }
            }

            if has_decorators(node, semantic) {
                continue;
            }

            // Async method/property of a class with an `implements` clause. The
            // interface controls the contract (commonly `(): Promise<T>`), and
            // `async` is the standard way to satisfy it without an explicit
            // return annotation, so the missing `await` is not a smell. Members
            // of a class without `implements`, and standalone functions, stay
            // flagged.
            if is_method_of_implementing_class(node, semantic) {
                continue;
            }

            // Async class method marked `override`. The `override` keyword binds
            // the method's signature to the parent class's contract (possibly an
            // external type such as the Cloudflare Workers `DurableObject`): when
            // the parent declares the method `async`, the override must stay
            // `async` to preserve the `Promise` return type under Liskov
            // substitution, so the missing `await` is not a smell. This is the
            // `extends`-only analog of `is_method_of_implementing_class`, since an
            // `extends`-only class has `ClassShape::has_implements == false`.
            if is_override_method(node, semantic) {
                continue;
            }

            // Async callback passed to a call (framework route handler, event
            // listener, etc.). The callee controls the contract: it frequently
            // requires a `() => Promise<T>` signature, and `async` is also
            // load-bearing for sync-throw safety (a synchronous `throw` becomes
            // a rejected Promise the framework handles uniformly). The author
            // does not own the call site, so flagging the missing `await` here
            // is noise. Standalone/named async functions stay flagged.
            if is_call_argument(node, semantic) {
                continue;
            }

            // Async function used as a JSX attribute value (`<form action={async
            // () => {}}>`). Same rationale as a call argument: the attribute's
            // prop type owns the contract (Next.js App Router `action` is typed
            // `() => Promise<void>`), so `async` is required even when the body
            // fires a bound server action without awaiting it.
            if is_jsx_attribute_value(node, semantic) {
                continue;
            }

            // Async property of an object literal passed to a call, whether a
            // shorthand method (`$config({ async run() {} })`) or an arrow value
            // (`useForm({ onSubmit: async () => {} })`). The callback's `async`
            // signature is mandated by the callee's type (`onSubmit: (...) =>
            // Promise<T>`) even when the body declares resources synchronously or
            // never awaits.
            if is_object_property_in_call_arg(node, semantic) {
                continue;
            }

            // Async function reassigned to `this.X` where the class declares an
            // `async X()` method (`this.flush = async () => {...}` overriding
            // `async flush()`). Callers do `await instance.X()`, so the
            // replacement must stay `async` to keep returning `Promise<void>`;
            // dropping it would break every `await` call site. The matching
            // `async` method ties the reassignment to the method's type contract.
            if is_async_method_override_on_this(node, semantic) {
                continue;
            }

            if has_await.contains(&node.id()) {
                continue;
            }

            // better-result: `Result.gen(async function* () { yield* Result.await(...) })`
            // The wrapping async has no direct `await` but is justified by the Result pipeline.
            let body_text = match node.kind() {
                AstKind::Function(f) => f.body.as_ref().map(|b| {
                    &ctx.source[b.span.start as usize..b.span.end as usize]
                }),
                AstKind::ArrowFunctionExpression(f) => {
                    Some(&ctx.source[f.body.span().start as usize..f.body.span().end as usize])
                }
                _ => None,
            };
            if let Some(text) = body_text {
                if text.contains("Result.await") || text.contains("Result.gen") {
                    continue;
                }
            }

            // Arrow with concise-body returning a value (`async () => X`).
            // The companion `promise-function-async` rule mandates the
            // `async` keyword whenever the surrounding type contract
            // expects a Promise — even when the body returns a constant
            // (`async () => EMPTY` to satisfy `(): Promise<T[]>`).
            // Flagging missing-await here makes the two rules impossible
            // to satisfy together. Skip any concise-body arrow.
            if let AstKind::ArrowFunctionExpression(arrow) = node.kind()
                && arrow.expression
            {
                continue;
            }

            // Block body that is either empty (`async () => {}`) or exactly
            // `return <call>();`. An empty body is a `Promise<void>` no-op whose
            // `async` is its only source of the return type — dropping it yields
            // `() => void` and breaks a contextual `(params) => Promise<void>`. A
            // single-return-call forwards another call's `Promise`. In both cases
            // `async` is load-bearing for the type contract (per
            // `promise-function-async`), so the absent `await` is not a smell.
            // These are the block-body analogs of the concise arrow exemption.
            let block_body = match node.kind() {
                AstKind::Function(f) => f.body.as_deref(),
                AstKind::ArrowFunctionExpression(f) if !f.expression => Some(&*f.body),
                _ => None,
            };
            if block_body.is_some_and(|body| {
                body.statements.is_empty() || body_is_single_return_call(body)
            }) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-async-without-await".into(),
                message: "`async` function never awaits — drop the `async` keyword \
                          or add the `await` that justifies it."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_on_tsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_result_await_pattern() {
        let src = r#"const run = async () => { return Result.gen(async function* () { const v = yield* Result.await(fetch()); return v; }); };"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_gen_pattern() {
        let src = r#"async function handler() { return Result.gen(async function* () { yield* Result.await(doStuff()); }); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_arrow_forwarding_promise_fn() {
        // Regression for rbaumier/comply#20 — `promise-function-async`
        // mandates async on Promise-returning arrows, then this rule
        // would trip on the missing await. Skip CallExpression bodies.
        assert!(run_on("const f = async () => fetch('/api');").is_empty());
        assert!(run_on("const g = async () => doStuff();").is_empty());
    }

    #[test]
    fn allows_async_arrow_returning_constant() {
        // Regression for rbaumier/comply#67 — concise-body arrow whose
        // expression is a non-call (constant / identifier / literal).
        // The async keyword can be load-bearing for the Promise return
        // type contract even when the body has no await.
        assert!(run_on("const f = async () => EMPTY;").is_empty());
        assert!(run_on("const f = async () => 42;").is_empty());
        assert!(run_on("const f = async () => [];").is_empty());
    }

    // Regression for #283: a no-op `Promise<void>` stub must be expressible as
    // an async function without tripping this rule — otherwise it contradicts
    // `promise-function-async` (which mandates the `async`). The delegated
    // `require-await`, which lacked these exceptions, was dropped in favour of
    // this rule.
    #[test]
    fn allows_empty_async_promise_void_stub() {
        assert!(run_on("async function noopAsync(): Promise<void> {}").is_empty());
    }

    #[test]
    fn allows_async_arrow_promise_void_stub() {
        assert!(run_on("const noopAsync = async (): Promise<void> => undefined;").is_empty());
    }

    #[test]
    fn allows_async_callback_passed_to_call() {
        // Regression for rbaumier/comply#1108 — async route handler registered
        // with a framework. The callee controls the contract and `async` is
        // intentional for sync-throw safety, so the missing await is not a smell.
        let src = r#"fastify.get("/v8/artifacts/status", async (_request, reply) => {
            return reply.send({ status: "enabled" });
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_callback_with_sync_throw() {
        // Second example from rbaumier/comply#1108 — a block-body async handler
        // whose only justification for `async` is sync-throw safety.
        let src = r#"fastify.post("/v8/artifacts/events", async (request, reply) => {
            if (!Array.isArray(request.body)) {
                throw new Error("Invalid request body.");
            }
            reply.code(200).send({});
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_run_method_in_sst_config() {
        // Regression for rbaumier/comply#1773 — SST's own project template.
        // `async run()` is a shorthand method in the object literal passed to
        // `$config(...)`; the framework types it `() => Promise<any>`, so async
        // is mandatory even though the body never awaits.
        let src = r#"export default $config({
            app(input) {
                return { name: "app", home: "aws" };
            },
            async run() {},
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_run_method_declaring_resources() {
        // Second example from rbaumier/comply#1773 — resources are declared via
        // synchronous constructor side effects, no await, but the framework
        // still requires the method to be async.
        let src = r#"export default $config({
            app(input) { return { name: "aws-workflow-python", home: "aws" }; },
            async run() {
                const workflow = new sst.aws.Workflow("Workflow", {});
                return { workflow: workflow.name };
            },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_arrow_value_in_call_options_object() {
        // Regression for rbaumier/comply#1600 — TanStack Form `onSubmit` callback.
        // The arrow is an object-property *value* (not a shorthand method) in the
        // options object passed to `useForm(...)`; the library types the property
        // `(...) => Promise<T>`, so `async` is required even with no await.
        let src = r#"const form = useForm({
            onSubmit: async ({ value }) => {
                console.log(value)
            },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_block_body_forwarding_call() {
        // Regression for rbaumier/comply#1600 — `FieldGroupApi`-style delegation.
        // A block body that is exactly `return <call>();` forwards another call's
        // Promise; `async` declares the Promise return type (per
        // `promise-function-async`), so the absent await is not a smell.
        let src = r#"class FieldGroupApi {
            validateArrayFieldsStartingFrom = async (field, index, cause) => {
                return this.form.validateArrayFieldsStartingFrom(field, index, cause);
            };
        }"#;
        assert!(run_on(src).is_empty());
        // Same shape as a standalone async function.
        assert!(run_on("async function f() { return delegate(); }").is_empty());
    }

    #[test]
    fn still_flags_async_block_body_returning_non_call() {
        // Negative space for #1600: a block body whose return is not a call (here
        // a member access) has no forwarded Promise to justify `async` — it stays
        // flagged. Guards the forwarding exemption against over-broadening.
        assert_eq!(run_on("async function f() { return this.value; }").len(), 1);
    }

    #[test]
    fn still_flags_async_arrow_value_outside_call() {
        // Negative space for #1600: an async arrow property value in a plain object
        // (not a call argument) has no callee contract — it stays flagged.
        let src = "const handlers = { onSubmit: async () => { doSync(); } };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_arrow_as_jsx_attribute_value() {
        // Regression for rbaumier/comply#2211 — Next.js App Router progressive
        // enhancement. The async arrow is a JSX attribute value (`action={...}`);
        // the `action` prop type contract requires `() => Promise<void>`, so
        // `async` is mandatory even though the body only fires a bound server
        // action without awaiting it. The author does not own the call site.
        let src = r#"function DeleteItemButton() {
            return (
                <form
                    action={async () => {
                        optimisticUpdate(merchandiseId, "delete");
                        removeItemAction();
                    }}
                >
                    <button type="submit">Delete</button>
                </form>
            );
        }"#;
        assert!(run_on_tsx(src).is_empty());
    }

    #[test]
    fn still_flags_async_arrow_outside_jsx_attribute() {
        // Negative space for #2211: an ordinary async function with no await that
        // is not a call argument nor a JSX attribute value has no external
        // contract — it stays flagged even in a .tsx file.
        assert_eq!(run_on_tsx("async function f() { return 1; }").len(), 1);
    }

    #[test]
    fn allows_async_method_of_implementing_class() {
        // Regression for rbaumier/comply#1678 — a class method marked `async` to
        // satisfy a Promise-returning interface method (Web API `Body.formData(): \
        // Promise<FormData>`), with no `await` and no explicit return annotation.
        let src = r#"class ElysiaRequest implements Body {
            async formData() {
                if (this.init?.body instanceof FormData) return this.init.body;
                throw new Error('Unable to parse body as FormData');
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_property_arrow_of_implementing_class() {
        // Second covered shape — an async class-property arrow (assigned to a
        // class field) in a class that declares `implements`.
        let src = r#"class Handler implements Contract {
            handle = async (x: number) => { return x; };
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_method_of_non_implementing_class() {
        // Negative space (a): an async method with no await in a class WITHOUT an
        // `implements` clause has no external contract to satisfy — stays flagged.
        let src = r#"class Plain {
            async formData() { return 42; }
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_nested_async_arrow_in_implementing_class_method() {
        // The exemption is gated on the function being a *direct* class member.
        // An async arrow nested inside a method body is not a class member, so it
        // stays flagged even though the enclosing class declares `implements`.
        let src = r#"class C implements I {
            async run() {
                const inner = async () => { return 1; };
                return inner;
            }
        }"#;
        // Only the nested arrow is flagged; the `run` method awaits nothing but is
        // exempt as a member of an implementing class.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_override_async_method_extends_only_class() {
        // Regression for rbaumier/comply#6564 — nitrojs/nitro
        // `$DurableObject extends DurableObject`. The `override` keyword binds the
        // method to the parent class's contract: the Cloudflare Workers
        // `DurableObject.webSocketMessage` is declared `async`/`Promise<void>`, so
        // the override must stay `async` even though the body never awaits.
        // The class uses `extends` only (no `implements`), so the interface
        // exemption does not fire — the `override` keyword carries the contract.
        let src = r#"export class $DurableObject extends DurableObject {
            override async webSocketMessage(client: WebSocket, message: ArrayBuffer | string) {
                if (import.meta._websocket) {
                    return ws!.handleDurableMessage(this, client, message);
                }
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_override_async_method_extends_only_class() {
        // Negative space for #6564: a plain (non-`override`) async method with no
        // await in an `extends`-only class has no parent-owned contract — it stays
        // flagged. Only the `override` keyword carries the exemption.
        let src = r#"class C extends Base {
            async foo() { return 1; }
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_top_level_async_function_alongside_override_exemption() {
        // Negative space for #6564: a top-level async function with no await is
        // unaffected by the `override` exemption and stays flagged.
        assert_eq!(run_on("async function bar() { return 1; }").len(), 1);
    }

    #[test]
    fn allows_override_async_method_with_await_unchanged() {
        // An `override async` method that does await is already passing via the
        // has-await path; confirm the new exemption does not change that.
        let src = r#"class C extends Base {
            override async baz() { await x(); }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_object_method_not_in_call() {
        // An object method outside any call argument is an ordinary async
        // function without await — it stays flagged.
        let src = "const obj = { async run() { return 42; } };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_method_nested_in_call_arg_subobject() {
        // Regression for rbaumier/comply#6807 — Pinia `defineStore` actions. The
        // async method is two object levels deep inside the call argument
        // (`fn -> ObjectProperty -> ObjectExpression(actions value) ->
        // ObjectProperty(actions) -> ObjectExpression(options) -> CallExpression`).
        // Pinia types every action `(...) => Promise<T>`, so `async` is mandatory
        // even when the body never awaits — same contract as a method directly in
        // the call-argument object.
        let src = r#"export const useTabbarStore = defineStore('core-tabbar', {
            actions: {
                async openTabInNewWindow(tab, router) {
                    const href = router.resolve(tab.fullPath || tab.path).href;
                    openWindow(new URL(href, location.href).href, { target: '_blank' });
                },
                async pinTab(tab) {
                    const index = this.tabs.findIndex((item) => equalTab(item, tab));
                    if (index === -1) return;
                    tab.meta.affixTab = true;
                },
            },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_method_three_levels_deep_in_call_arg() {
        // The walk climbs an arbitrary number of `ObjectProperty -> ObjectExpression`
        // hops, so a method nested three object levels deep in a call argument is
        // also exempt.
        let src = r#"configure({
            a: { b: { async run() { doSync(); } } },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_method_nested_in_plain_object() {
        // Negative space for #6807: a method nested in sub-objects of a *plain*
        // object literal (not a call argument) has no callee contract — the chain
        // climbs `ObjectProperty -> ObjectExpression -> ObjectProperty ->
        // ObjectExpression -> VariableDeclarator`, which is not a call argument, so
        // it stays flagged.
        let src = "const store = { actions: { async run() { return 1; } } };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_iife_without_await() {
        // An immediately-invoked async arrow is the callee, not an argument, so
        // it is not a framework callback and stays flagged.
        let src = "(async () => { return 42; })();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_block_body_async_arrow() {
        // Regression for rbaumier/comply#3850 — drizzle's `invalidate: async
        // (_params) => {}`. An empty-block-body async is a `Promise<void>` no-op
        // whose `async` is its only source of the return type; dropping it would
        // make it `() => void` and break a contextual `(params) => Promise<void>`.
        // Block-body analog of the concise-constant arrow and the annotated stub.
        assert!(run_on("const f = async (_params) => {};").is_empty());
    }

    #[test]
    fn allows_empty_block_body_async_function() {
        // Same shape as a standalone async function declaration with an empty body.
        assert!(run_on("async function noop() {}").is_empty());
    }

    #[test]
    fn allows_async_arrow_override_of_async_class_method() {
        // Regression for rbaumier/comply#6215 — sindresorhus/got `Request.flush`.
        // The async arrow is reassigned to `this.flush`, which the class declares
        // as `async flush()`. Callers do `await request.flush()`, so the
        // replacement must stay async (return `Promise<void>`); dropping `async`
        // would change the return type to `void` and break every await call site.
        let src = r#"class Request extends Duplex {
            async flush() {
                if (this._flushed) { return; }
                this._flushed = true;
                await this._doFlush();
            }
            _init() {
                this.flush = async () => {
                    this.flush = async () => {};
                    process.nextTick(() => {
                        this._beforeError(new Error("x"));
                    });
                };
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_this_override_without_matching_async_method() {
        // Negative space for #6215: `this.foo = async () => {...}` with NO
        // `async foo()` declared in the class has no method contract to preserve
        // — it stays flagged.
        let src = r#"class C {
            _init() {
                this.foo = async () => { doSync(); };
            }
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_override_on_non_this_target() {
        // Negative space for #6215: `obj.foo = async () => {...}` targets an
        // external object, not `this`, even when a same-named async method exists
        // — the class-method contract does not apply, so it stays flagged.
        let src = r#"class C {
            async foo() { await work(); }
            _init() {
                obj.foo = async () => { doSync(); };
            }
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_this_override_when_method_not_async() {
        // Negative space for #6215: `this.foo = async () => {...}` where the class
        // declares a *non-async* `foo()` has no Promise contract mandating
        // `async` — it stays flagged.
        let src = r#"class C {
            foo() { return 1; }
            _init() {
                this.foo = async () => { doSync(); };
            }
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_arrow_bound_to_promise_annotation() {
        // Regression for rbaumier/comply#6566 — nitrojs/nitro
        // `public-assets.ts`. The async arrow has no own return annotation, but
        // its binding declares `(id: string) => Promise<Buffer>`. Without
        // `async`, the sync-throw body's inferred return type is `never`, which
        // does not satisfy `Promise<Buffer>`, so `async` is mandatory even though
        // the body never awaits.
        let src = r#"export const readAsset: (id: string) => Promise<Buffer> = async () => {
            throw new Error("Asset not found");
        };"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_arrow_bound_to_non_promise_annotation() {
        // Negative space for #6566: a binding annotation that is NOT
        // Promise-returning (`() => number`) gives `async` no type contract to
        // satisfy — the arrow stays flagged.
        let src = "const f: () => number = async () => { return 1; };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_arrow_without_annotation() {
        // Negative space for #6566: an async arrow with no binding annotation and
        // no await stays flagged.
        let src = "const f = async () => { return 1; };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_arrow_asserted_to_inline_promise_type() {
        // Inline-Promise variant of #6583: the arrow is asserted to a function
        // type whose call signature returns `Promise<void>`. Without `async`,
        // the body's inferred return type is `void`, not assignable to the
        // asserted `() => Promise<void>`, so `async` is load-bearing.
        let src = r#"const unregister = (async () => {
            doSync();
            cleanup();
        }) as () => Promise<void>;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_arrow_asserted_to_named_promise_alias() {
        // Regression for rbaumier/comply#6583 — privatenumber/tsx
        // `register.ts`. The async arrow is asserted `as NamespacedUnregister`,
        // a named alias that resolves (through in-file aliases) to
        // `() => Promise<void>`. Without `async`, the body's inferred return
        // type is `void`, not assignable to the asserted Promise-returning
        // signature, so `async` is mandatory even though the body never awaits.
        let src = r#"type Unregister = () => Promise<void>;
        type NamespacedUnregister = Unregister & {
            import: ScopedImport;
            unregister: Unregister;
        };
        function register() {
            const unregister = (async () => {
                hookData.active = false;
                registeredHooks.deregister();
            }) as NamespacedUnregister;
            return unregister;
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_arrow_satisfies_promise_type() {
        // The `satisfies` operator carries the same contract as `as`.
        let src = r#"const handler = (async () => {
            doSync();
        }) satisfies () => Promise<void>;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_arrow_asserted_to_non_promise_type() {
        // Negative space for #6583: an asserted type whose call signature does
        // not return a Promise gives `async` no contract to satisfy — the arrow
        // stays flagged.
        let src = r#"const f = (async () => {
            return 1;
        }) as () => number;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_arrow_asserted_to_named_non_promise_alias() {
        // Negative space for #6583: a named alias that does NOT resolve to a
        // Promise-returning type leaves the diagnostic firing.
        let src = r#"type SyncFn = () => number;
        const f = (async () => {
            return 1;
        }) as SyncFn;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_arrow_asserted_to_promise_through_double_parens() {
        // The operand may be wrapped in more than one layer of parentheses; the
        // assertion still owns the contract.
        let src = r#"const f = ((async () => {
            doSync();
        })) as () => Promise<void>;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_arrow_asserted_to_non_ascii_named_alias() {
        // Guards alias-name scanning against non-ASCII identifier names: the
        // asserted type `XΩmega` is a sync alias and must stay flagged, and
        // scanning it for the unrelated `Ωmega` alias must not panic on the
        // multi-byte boundary.
        let src = r#"type Ωmega = () => Promise<void>;
        type XΩmega = () => number;
        const f = (async () => {
            doSync();
        }) as XΩmega;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_function_body_without_await() {
        let src = "async function f() { return 42; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_without_await() {
        let d = run_on("async function f() { return 42; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_async_generator_without_await() {
        // Regression for #7123 — `async function*` sets the return type to
        // `AsyncIterable<T>`, so `async` is load-bearing even with no await.
        let src = "async function* gen(): AsyncIterable<number> { yield 1; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_generator_with_await() {
        // A generator is skipped regardless of whether the body awaits.
        let src = "async function* gen() { await foo(); yield 1; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_generator_method_without_await() {
        let src = "class C { async *m() { yield 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_generator_async_arrow_without_await() {
        // Control: a plain async arrow with no await stays flagged.
        assert_eq!(run_on("const f = async () => { doSync(); };").len(), 1);
    }
}
