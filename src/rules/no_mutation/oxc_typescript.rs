//! no-mutation OXC backend — flag mutations on `const` bindings.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_constant_index_expression, is_get_context_call_binding,
    is_local_dispatch_table_binding, is_local_object_builder_binding,
    is_locally_owned_array_binding, is_react_display_name_assignment, is_rtk_reducer_draft_param,
    is_typed_array_binding, is_valtio_proxy_binding, is_vue_ref_value_target,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, Expression, IdentifierReference, PropertyKey, UnaryOperator,
    VariableDeclarationKind,
};
use std::sync::Arc;

const MUTATING_ARRAY_METHODS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

const OBJECT_MUTATOR_FUNCTIONS: &[&str] = &[
    "assign",
    "defineProperty",
    "defineProperties",
    "setPrototypeOf",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::AssignmentExpression,
            AstType::UpdateExpression,
            AstType::UnaryExpression,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Test files mutate const restore buffers and `process.env` to inject
        // and reset environment-variable state across cases — the canonical
        // test-time injection surface with no non-mutating alternative.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // Storybook CSF2 attaches story metadata (args, storyName, play,
        // parameters, decorators) by assigning to named properties on the
        // exported story function — the designed API with no immutable
        // alternative; the runner reads these off the function.
        if ctx.file.path_segments.in_storybook {
            return;
        }
        // Sentry's beforeSend/beforeBreadcrumb hooks receive the event by
        // reference, expect in-place mutation, and return the same object —
        // there is no immutable alternative API.
        if is_inside_sentry_hook(node, semantic) {
            return;
        }
        match node.kind() {
            // obj.prop = x, obj.prop += x
            AstKind::AssignmentExpression(assign) => {
                // ref.current = ... (React useRef pattern)
                if is_current_target(&assign.left) {
                    return;
                }
                // Component.displayName = "Component" (React naming convention)
                if is_react_display_name_assignment(assign) {
                    return;
                }
                // Vue 3 reactive ref: `count.value = x` drives reactivity. Also
                // covers a `Ref<T>` destructured from a composable call
                // (`const { error } = useThing(); error.value = x`).
                if let AssignmentTarget::StaticMemberExpression(member) = &assign.left
                    && (is_vue_ref_value_target(member, semantic, ctx.project, ctx.path)
                        || crate::oxc_helpers::is_destructured_call_ref_value_target(
                            member, semantic,
                        ))
                {
                    return;
                }
                // TypedArray element write `buf[i] = v`: indexed assignment is the
                // only way to populate a TypedArray (a fixed-length binary buffer
                // with no immutable element-setter and no spread-then-build form).
                if is_typed_array_element_target(&assign.left, semantic) {
                    return;
                }
                // Sparse dispatch-table construction: `const handlers = [];
                // handlers[0x01] = fn` builds a locally-owned lookup table by
                // constant-index assignment — array construction, not mutation.
                if is_dispatch_table_element_target(&assign.left, semantic) {
                    return;
                }
                if let Some(id) = root_identifier_of_target(&assign.left)
                    && (is_created_dom_element(id, semantic)
                        || is_local_object_builder_binding(id, semantic)
                        || is_rtk_reducer_draft_param(id, semantic)
                        || is_valtio_proxy_binding(id, semantic)
                        || is_get_context_call_binding(id, semantic))
                {
                    return;
                }
                let Some(root) = root_name_of_target(&assign.left) else {
                    return;
                };
                if is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, assign.span.start, root, "Mutating property of");
                }
            }
            // obj.count++, --obj.count
            AstKind::UpdateExpression(update) => {
                // Vue 3 reactive ref: `count.value++` drives reactivity. Also
                // covers a `Ref<T>` destructured from a composable call.
                if let oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(member) =
                    &update.argument
                    && (is_vue_ref_value_target(member, semantic, ctx.project, ctx.path)
                        || crate::oxc_helpers::is_destructured_call_ref_value_target(
                            member, semantic,
                        ))
                {
                    return;
                }
                // TypedArray element update `buf[i]++`: same in-place-write idiom.
                if is_typed_array_element_simple_target(&update.argument, semantic) {
                    return;
                }
                if let Some(id) = root_identifier_of_simple_target(&update.argument)
                    && (is_created_dom_element(id, semantic)
                        || is_rtk_reducer_draft_param(id, semantic)
                        || is_valtio_proxy_binding(id, semantic)
                        || is_get_context_call_binding(id, semantic))
                {
                    return;
                }
                let Some(root) = root_name_of_simple_target(&update.argument) else {
                    return;
                };
                if is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, update.span.start, root, "Mutating property of");
                }
            }
            // delete obj.prop
            AstKind::UnaryExpression(unary) => {
                if unary.operator != UnaryOperator::Delete {
                    return;
                }
                if let Some(id) = root_identifier_of_expr(&unary.argument)
                    && (is_created_dom_element(id, semantic)
                        || is_rtk_reducer_draft_param(id, semantic)
                        || is_valtio_proxy_binding(id, semantic)
                        || is_get_context_call_binding(id, semantic))
                {
                    return;
                }
                let Some(root) = root_name_of_expr(&unary.argument) else {
                    return;
                };
                if is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, unary.span.start, root, "Deleting property of");
                }
            }
            // arr.push(x), Object.assign(obj, ...)
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                let method = member.property.name.as_str();

                // Object.assign(target, ...)
                if OBJECT_MUTATOR_FUNCTIONS.contains(&method) {
                    if let Expression::Identifier(obj) = &member.object
                        && obj.name.as_str() == "Object"
                            && let Some(first_arg) = call.arguments.first() {
                                // Skip `Object.assign(fn, { ...literal })` — attaching a
                                // static property to a function. JS has no immutable
                                // alternative; see rbaumier/comply#154.
                                if method == "assign"
                                    && is_assign_static_to_function(call, semantic)
                                {
                                    return;
                                }
                                let root = match first_arg.as_expression() {
                                    Some(Expression::Identifier(ident)) => {
                                        Some(ident.name.as_str())
                                    }
                                    Some(expr) => root_name_of_expr(expr),
                                    None => None,
                                };
                                if let Some(root) = root
                                    && is_declared_as_const(semantic, root) {
                                        report(
                                            diagnostics,
                                            ctx,
                                            call.span.start,
                                            root,
                                            "Mutating",
                                        );
                                    }
                            }
                    return;
                }

                if !MUTATING_ARRAY_METHODS.contains(&method) {
                    return;
                }

                // `state.ids.push(…)` mutates an intentional-mutation target: a
                // Redux Toolkit reducer's Immer draft (the documented RTK pattern,
                // not aliased state) or a valtio `proxy()` binding (direct mutation
                // is valtio's entire API).
                if let Some(id) = root_identifier_of_expr(&member.object)
                    && (is_rtk_reducer_draft_param(id, semantic)
                        || is_valtio_proxy_binding(id, semantic))
                {
                    return;
                }

                let root = match &member.object {
                    Expression::Identifier(ident) => Some(ident.name.as_str()),
                    expr => root_name_of_expr(expr),
                };
                let Some(root) = root else {
                    return;
                };

                // Skip `.push()` / `.unshift()` on a const local
                // accumulator inside a loop body — a common,
                // bounded, escape-free pattern. The structurally
                // correct alternative (`Result.all`) is missing from
                // better-result: tracking dmmulroy/better-result#32.
                //
                // Same exemption inside a `Result.gen(function*() { ... })`
                // block — the generator body is the canonical
                // accumulator site for sequencing `yield*` results,
                // and the spread alternative breaks short-circuiting
                // on the first error.
                if matches!(method, "push" | "unshift")
                    && matches!(&member.object, Expression::Identifier(_))
                    && (is_inside_loop_body(node, semantic)
                        || is_inside_result_gen(node, semantic))
                {
                    return;
                }

                // Skip `.push()` / `.unshift()` on a locally-owned fresh array —
                // a `VariableDeclarator` array-literal (or `new Array(...)`)
                // binding in a non-module scope — regardless of loop context.
                // Nothing outside the declaring function observes the mutation
                // (the "build a local array, then return/consume it" pattern), so
                // it is not the shared-state mutation this rule targets. Mirrors
                // the sibling `no-mutating-methods` exemption. A parameter,
                // module-scope, or member-expression receiver is not locally
                // owned and stays flagged.
                if matches!(method, "push" | "unshift")
                    && let Expression::Identifier(receiver) = &member.object
                    && is_locally_owned_array_binding(receiver, semantic)
                {
                    return;
                }

                if is_declared_as_const(semantic, root) {
                    report(
                        diagnostics,
                        ctx,
                        call.span.start,
                        root,
                        &format!("Calling `{method}()` on"),
                    );
                }
            }
            _ => {}
        }
    }
}

const SENTRY_HOOKS: &[&str] = &["beforeSend", "beforeBreadcrumb", "beforeSendTransaction"];

/// Static name of an object-property key, if it's an identifier or string literal.
fn static_key_name<'a>(key: &PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Name of the nearest enclosing named function (declaration or named expression).
fn nearest_enclosing_fn_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::Function(func) = ancestor.kind()
            && let Some(id) = &func.id
        {
            return Some(id.name.as_str());
        }
    }
    None
}

/// True when the mutation sits inside a Sentry hook callback — either an inline
/// lambda/method assigned to `beforeSend`/`beforeBreadcrumb`/`beforeSendTransaction`,
/// or a named function registered as one of those hooks somewhere in the file.
fn is_inside_sentry_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
        {
            return true;
        }
    }

    let Some(fn_name) = nearest_enclosing_fn_name(node, semantic) else {
        return false;
    };
    for n in semantic.nodes().iter() {
        if let AstKind::ObjectProperty(prop) = n.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
            && let Expression::Identifier(id) = &prop.value
            && id.name.as_str() == fn_name
        {
            return true;
        }
    }
    false
}

fn is_current_target(target: &AssignmentTarget) -> bool {
    match target {
        AssignmentTarget::StaticMemberExpression(member) => {
            member.property.name.as_str() == "current"
        }
        _ => false,
    }
}

/// Extract the root identifier name from an assignment target (must be member access).
fn root_name_of_target<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a str> {
    match target {
        // Plain identifier = reassignment, not property mutation
        AssignmentTarget::AssignmentTargetIdentifier(_) => None,
        AssignmentTarget::StaticMemberExpression(member) => root_name_of_expr(&member.object),
        AssignmentTarget::ComputedMemberExpression(member) => root_name_of_expr(&member.object),
        _ => None,
    }
}

fn root_name_of_simple_target<'a>(
    target: &'a oxc_ast::ast::SimpleAssignmentTarget<'a>,
) -> Option<&'a str> {
    match target {
        oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(m) => {
            root_name_of_expr(&m.object)
        }
        oxc_ast::ast::SimpleAssignmentTarget::ComputedMemberExpression(m) => {
            root_name_of_expr(&m.object)
        }
        _ => None,
    }
}

fn root_name_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => root_name_of_expr(&member.object),
        Expression::ComputedMemberExpression(member) => root_name_of_expr(&member.object),
        _ => None,
    }
}

fn root_identifier_of_target<'a>(
    target: &'a AssignmentTarget<'a>,
) -> Option<&'a IdentifierReference<'a>> {
    match target {
        AssignmentTarget::StaticMemberExpression(m) => root_identifier_of_expr(&m.object),
        AssignmentTarget::ComputedMemberExpression(m) => root_identifier_of_expr(&m.object),
        _ => None,
    }
}

fn root_identifier_of_simple_target<'a>(
    target: &'a oxc_ast::ast::SimpleAssignmentTarget<'a>,
) -> Option<&'a IdentifierReference<'a>> {
    match target {
        oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(m) => {
            root_identifier_of_expr(&m.object)
        }
        oxc_ast::ast::SimpleAssignmentTarget::ComputedMemberExpression(m) => {
            root_identifier_of_expr(&m.object)
        }
        _ => None,
    }
}

fn root_identifier_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a IdentifierReference<'a>> {
    match expr {
        Expression::Identifier(id) => Some(id),
        Expression::StaticMemberExpression(m) => root_identifier_of_expr(&m.object),
        Expression::ComputedMemberExpression(m) => root_identifier_of_expr(&m.object),
        _ => None,
    }
}

/// True when `target` is a computed-member write `base[i]` whose `base` is a
/// direct identifier resolving to a TypedArray binding — `buf[i] = v`. Indexed
/// element assignment is the only way to write a TypedArray's contents, so this
/// element write has no immutable alternative. A static-member write (`buf.length`)
/// or a deeper chain (`obj.buf[i]`) is not exempt — only the direct indexed write.
fn is_typed_array_element_target(
    target: &AssignmentTarget,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AssignmentTarget::ComputedMemberExpression(member) = target else {
        return false;
    };
    matches!(
        &member.object,
        Expression::Identifier(id) if is_typed_array_binding(id, semantic)
    )
}

/// True when `target` is a constant-keyed indexed write into a freshly-constructed,
/// locally-owned array — `const handlers = []; handlers[0x01] = fn` — i.e. a sparse
/// dispatch/lookup table being built. The base must be a direct identifier resolving
/// to a local empty-array `const` binding, and the index a constant key (numeric
/// literal or `const` opcode). A dynamic index, a foreign or parameter array, or a
/// deeper chain (`obj.table[k]`) does not match, so post-construction or shared-state
/// mutation stays flagged.
fn is_dispatch_table_element_target(
    target: &AssignmentTarget,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AssignmentTarget::ComputedMemberExpression(member) = target else {
        return false;
    };
    matches!(
        &member.object,
        Expression::Identifier(id) if is_local_dispatch_table_binding(id, semantic)
    ) && is_constant_index_expression(&member.expression, semantic)
}

/// [`is_typed_array_element_target`] for an `UpdateExpression`'s
/// `SimpleAssignmentTarget` (`buf[i]++`).
fn is_typed_array_element_simple_target(
    target: &oxc_ast::ast::SimpleAssignmentTarget,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let oxc_ast::ast::SimpleAssignmentTarget::ComputedMemberExpression(member) = target else {
        return false;
    };
    matches!(
        &member.object,
        Expression::Identifier(id) if is_typed_array_binding(id, semantic)
    )
}

/// True when `ident` resolves to a binding initialised via `document.createElement(...)`
/// or `document.createElementNS(...)`. A freshly created DOM element is unattached and
/// must be configured by property assignment before insertion — not a state mutation.
fn is_created_dom_element(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(init) = &decl.init else { return false };
            return is_create_element_call(init);
        }
    }
    false
}

fn is_create_element_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let Expression::Identifier(obj) = &member.object else { return false };
    if obj.name.as_str() != "document" { return false }
    let method = member.property.name.as_str();
    method == "createElement" || method == "createElementNS"
}

/// Check if a name is declared as `const` in the current scope chain.
fn is_declared_as_const(semantic: &oxc_semantic::Semantic, name: &str) -> bool {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();

    for sym_id in scoping.symbol_ids() {
        if scoping.symbol_name(sym_id) != name {
            continue;
        }
        let decl_node_id = scoping.symbol_declaration(sym_id);
        // Walk up to find VariableDeclaration with const kind
        for kind in nodes.ancestor_kinds(decl_node_id) {
            match kind {
                AstKind::VariableDeclaration(decl) => {
                    return decl.kind == VariableDeclarationKind::Const;
                }
                AstKind::FormalParameter(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_) => {
                    return false;
                }
                _ => continue,
            }
        }
    }
    false
}

/// True if `node` sits inside a `for` / `for-of` / `for-in` / `while`
/// loop body, stopping at function boundaries. Used to recognise the
/// bounded local-accumulator pattern (`const items = []; for (...)
/// items.push(...);`) as a deliberate, escape-free mutation.
fn is_inside_loop_body(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `node` lives inside the generator function passed to
/// `Result.gen(function*() { ... })` (or an arrow form). The generator
/// body sequences `yield*` results into a local array — that's the
/// canonical accumulator site, and the spread alternative breaks
/// short-circuiting on the first error.
fn is_inside_result_gen(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) if func.generator => {
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind()
                    && is_result_gen_callee(&call.callee)
                {
                    return true;
                }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind()
                    && is_result_gen_callee(&call.callee)
                {
                    return true;
                }
                return false;
            }
            _ => {}
        }
    }
    false
}

/// True when `call` is `Object.assign(fn, { ...literal })` where `fn` is
/// an identifier bound to a `const`-declared function/arrow expression.
/// Recognises the JS-canonical "attach static prop to a function" pattern.
fn is_assign_static_to_function(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(first) = call.arguments.first() else { return false };
    let Some(second) = call.arguments.get(1) else { return false };

    if !matches!(second, oxc_ast::ast::Argument::ObjectExpression(_)) {
        return false;
    }

    let oxc_ast::ast::Argument::Identifier(ident) = first else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        if let AstKind::VariableDeclarator(decl) = kind {
            return matches!(
                decl.init,
                Some(Expression::ArrowFunctionExpression(_))
                    | Some(Expression::FunctionExpression(_)),
            );
        }
    }
    false
}

fn is_result_gen_callee(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    matches!(obj.name.as_str(), "Result" | "Effect") && member.property.name.as_str() == "gen"
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

    /// Build a temp project from `(rel_path, source)` pairs, index it so the
    /// cross-file `ImportIndex` is populated, and run the rule on `target_rel`.
    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> Vec<Diagnostic> {
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        let target_path = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let canon = fs::canonicalize(&target_path).unwrap();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_oxc_check(&Check, &source, &canon, &project, file)
    }

    #[test]
    fn ignores_push_inside_result_gen_with_loop() {
        // Regression for rbaumier/comply#23 — canonical Result.gen accumulator.
        let src = r#"
            function mapResults(items, fn) {
                return Result.gen(function* () {
                    const mapped = [];
                    for (const item of items) {
                        mapped.push(yield* fn(item));
                    }
                    return mapped;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_result_gen_without_loop() {
        // Regression for rbaumier/comply#23 — sequential yields inside Result.gen.
        let src = r#"
            function fetchAll() {
                return Result.gen(function* () {
                    const out = [];
                    out.push(yield* loadUser());
                    out.push(yield* loadOrders());
                    return out;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_typed_accumulator_two_step_yield_in_result_gen() {
        // Regression for rbaumier/comply#363 — exact amadeo pattern:
        // type-annotated const, two-step (separate yield + push), Result.ok wrapper.
        let src = r#"
            type User = { id: string };
            function getUsers(rows: unknown[], orgId: string) {
                return Result.gen(function* () {
                    const items: User[] = [];
                    for (const row of rows) {
                        const user = yield* rowToUser(row as any, orgId);
                        items.push(user);
                    }
                    return Result.ok({ items, total: items.length });
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_effect_gen_without_loop() {
        // Effect.gen (effect-ts) uses the same sequential-yield accumulator
        // pattern and must be treated the same as Result.gen.
        let src = r#"
            type User = { id: string };
            function fetchTwo() {
                return Effect.gen(function* () {
                    const users: User[] = [];
                    const u1 = yield* fetchUser("id1");
                    users.push(u1);
                    const u2 = yield* fetchUser("id2");
                    users.push(u2);
                    return users;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_object_assign_attaching_static_to_function() {
        // Regression for rbaumier/comply#154 — Object.assign on a function
        // const with an object literal is the canonical static-prop pattern.
        let src = r#"
            const defaults = { mode: "strict" };
            const parser = (input: unknown) => input;
            return Object.assign(parser, { defaults });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_object_assign_on_plain_const() {
        let src = r#"
            const target = { a: 1 };
            Object.assign(target, { b: 2 });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_assignment_on_local_object_spread_builder() {
        // Regression for rbaumier/comply#1930 — dnd-kit boundingRectangle:
        // `value` is a fresh local copy via object spread, built up via
        // conditional property assignments before being returned. No external
        // state is mutated — the object analogue of the array accumulator.
        let src = r#"
            export function boundingRectangle(transform, shape, boundingRect) {
                const value = { ...transform };
                if (cond) {
                    value.y = boundingRect.top - shape.boundingRectangle.top;
                } else if (cond2) {
                    value.y = boundingRect.bottom;
                }
                if (cond3) {
                    value.x = boundingRect.left;
                }
                return value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_local_object_literal_builder() {
        let src = r#"
            function build() {
                const value = { a: 1 };
                value.b = 2;
                return value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_assignment_on_const_from_external_call() {
        // A `const` initialized from a function call (not an object literal /
        // spread) references external state — mutating it is still flagged.
        let src = r#"
            function mutate() {
                const value = getConfig();
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_assignment_on_created_dom_element() {
        let src = r#"
            function download(objectUrl: string, filename: string) {
                const anchor = document.createElement("a");
                anchor.href = objectUrl;
                anchor.download = filename;
                anchor.rel = "noopener";
                document.body.append(anchor);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_created_svg_element() {
        let src = r#"
            function build() {
                const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                svg.id = "chart";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutation_on_unrelated_const() {
        let src = r#"
            function set(objectUrl: string) {
                const anchor = getAnchorFromDom();
                anchor.href = objectUrl;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Canvas rendering-context property assignment — issue #2277

    #[test]
    fn allows_property_assignment_on_get_context_binding_issue_2277() {
        // Regression for rbaumier/comply#2277 — a CanvasRenderingContext2D from
        // `canvas.getContext('2d')` is an imperative stateful API; setting
        // `fillStyle`/`lineWidth`/etc. is the only way to use it, no immutable
        // alternative exists.
        let src = r#"
            const ctx = canvas.getContext('2d');
            ctx.fillStyle = 'red';
            ctx.lineWidth = 2;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_non_null_get_context_binding() {
        // The issue's exact shape uses a non-null assertion on the call.
        let src = r#"
            const context = canvas.getContext('2d')!;
            context.fillStyle = gradient;
            context.globalAlpha = 0.5;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_assignment_on_ordinary_const_object_2277() {
        // Negative space: a const not derived from getContext references
        // external state — mutating it stays flagged.
        let src = r#"
            const o = makeThing();
            o.value = 5;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sentry beforeSend/beforeBreadcrumb in-place scrub hooks — issue #478

    #[test]
    fn allows_const_mutation_inside_inline_before_breadcrumb_method() {
        let src = r#"
            Sentry.init({
                beforeBreadcrumb(breadcrumb) {
                    const data = breadcrumb.data;
                    data.url = scrubSensitiveQueryFromUrl(data.url);
                    return breadcrumb;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_mutation_in_named_function_registered_as_before_send() {
        let src = r#"
            function scrubEvent(event) {
                const req = event.request;
                req.url = scrubSensitiveQueryFromUrl(req.url);
                return event;
            }
            Sentry.init({ beforeSend: scrubEvent });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_const_mutation_outside_sentry_hook() {
        let src = r#"
            function scrub() {
                const data = getData();
                data.url = scrubSensitiveQueryFromUrl(data.url);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // React displayName naming convention — issue #1779

    #[test]
    fn allows_display_name_assignment_on_forward_ref_component() {
        // Regression for rbaumier/comply#1779 — setting `displayName` on a
        // forwardRef-wrapped component is the standard React naming convention.
        let src = r#"
            const RadioGroup = React.forwardRef((props, ref) => {
                return <RadioGroupPrimitives.Root ref={ref} {...props} />;
            });
            RadioGroup.displayName = "RadioGroup";
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn still_flags_non_string_display_name_assignment() {
        // Only string-literal `displayName` writes are exempt; assigning a
        // computed value to a const's property is still a mutation.
        let src = r#"
            const RadioGroup = React.forwardRef((props, ref) => null);
            RadioGroup.displayName = getName();
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_other_string_property_assignment() {
        let src = r#"
            const RadioGroup = React.forwardRef((props, ref) => null);
            RadioGroup.label = "RadioGroup";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Storybook CSF2 story-file exemption — issue #1680

    fn storybook_file_ctx() -> crate::rules::file_ctx::FileCtx {
        crate::rules::file_ctx::FileCtx {
            path_segments: crate::rules::file_ctx::PathSegments {
                in_storybook: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn allows_csf2_story_property_assignments() {
        // Regression for rbaumier/comply#1680 — CSF2 attaches story metadata
        // via property assignment on the exported story function. No immutable
        // alternative exists; story files must not be flagged.
        let src = r#"
            export const WithArgs = (args) => <Button {...args} />;
            WithArgs.args = { label: "With args" };
            WithArgs.storyName = "With args";
            WithArgs.play = () => { /* interaction test */ };
        "#;
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(
                &Check,
                src,
                "Button.stories.tsx",
                crate::project::default_static_project_ctx(),
                &storybook_file_ctx(),
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_same_pattern_in_non_story_file() {
        // Negative space: the identical property-assignment-on-const pattern
        // in a non-story file is still a mutation and must fire.
        let src = r#"
            const WithArgs = (args) => null;
            WithArgs.args = { label: "With args" };
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Array.reduce() accumulator — issue #2239

    #[test]
    fn allows_mutation_on_reduce_accumulator_issue_2239() {
        // Regression for rbaumier/comply#2239 — pinia mapHelpers: the reduce
        // accumulator is a fresh local object literal passed as the seed; it
        // never escapes until `reduce` returns, so building it up via property
        // assignment is the canonical reduce-to-object pattern.
        let src = r#"
            function build(stores, suffix) {
                return stores.reduce((reduced, useStore) => {
                    reduced[useStore.$id + suffix] = function () {
                        return useStore();
                    };
                    return reduced;
                }, {});
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_const_mutation_on_non_accumulator_inside_reduce() {
        // Negative space: a `const` declared inside a reduce callback (not the
        // accumulator parameter) references whatever it was initialised from —
        // mutating it stays flagged.
        let src = r#"
            arr.reduce((reduced, item) => {
                const cfg = getConfig();
                cfg.x = 1;
                return reduced;
            }, {});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_const_mutation_on_accumulator_of_non_reduce_call() {
        // Negative space: the callback of a non-`.reduce()` call has no local
        // accumulator; mutating a const inside it stays flagged.
        let src = r#"
            arr.forEach((acc, item) => {
                const cfg = getConfig();
                cfg.x = item;
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue 3 reactive ref `.value` mutation — issue #2164

    #[test]
    fn allows_vue_ref_value_mutation_issue_2164() {
        // Regression for rbaumier/comply#2164 — `ref()` returns a reactive
        // wrapper whose `.value` assignment/update is the intended mutation
        // point that drives Vue's reactivity.
        let src = r#"
            import { ref } from 'vue'
            const count = ref(0)
            const input = ref('')
            function update(e) {
                count.value++
                input.value = e.target.value
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_mutation_on_plain_const_without_vue_import() {
        // Negative space: `.value` on a const from an external call (not a vue
        // ref factory, no vue import) is a genuine mutation and stays flagged.
        let src = r#"
            const plain = getThing();
            plain.value = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_value_property_mutation_on_vue_ref() {
        // Negative space: only `.value` is the reactive mutation point; writing
        // any other property on a ref is still a mutation.
        let src = r#"
            import { ref } from 'vue'
            const r = ref(0);
            r.config = 5;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue ref via destructured composable / Ref-typed param / imported ref — issue #7603

    #[test]
    fn allows_value_mutation_on_composable_destructured_ref_issue_7603() {
        // Parity with no-property-mutation: a `Ref<T>` destructured from a
        // composable call is mutated only through `.value`.
        let src = r#"
            const { queryClicks } = useNav();
            function bump() { queryClicks.value += 1; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_ref_typed_parameter_issue_7603() {
        // A `Ref<T>` / `ModelRef<T>` parameter is a ref by its type annotation;
        // `.value` assignment is the intended reactive update.
        let src = r#"
            function useNavBase(queryClicks: Ref<number>) {
                queryClicks.value += 1;
            }
            export function useIME(content: ModelRef<string>) {
                content.value = 'x';
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_value_property_on_composable_destructured_binding_issue_7603() {
        // Negative space for the parity fallback: the destructured-composable
        // exemption is `.value`-restricted, so a non-`.value` property write on a
        // const call-destructured binding stays flagged.
        let src = r#"
            const { cfg } = useThing();
            cfg.enabled = true;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_value_mutation_on_imported_ref_issue_7603() {
        // A ref exported from a centralized state module; `.value =` on the
        // imported binding is a reactive write.
        let files = &[
            (
                "state/index.ts",
                "import { ref } from 'vue'\nexport const hmrSkipTransition = ref(false)",
            ),
            (
                "composables/useNav.ts",
                "import { hmrSkipTransition } from '../state'\n\
                 export function useNav() { hmrSkipTransition.value = false; }",
            ),
        ];
        assert!(run_on_project(files, "composables/useNav.ts").is_empty());
    }

    // TypedArray indexed element assignment — issue #5328

    #[test]
    fn allows_typed_array_element_assignment_issue_5328() {
        // Regression for rbaumier/comply#5328 — pdf-lib pdfDocEncoding: a
        // Uint16Array lookup table populated by indexed writes during module
        // init. Indexed assignment is the only way to write a TypedArray's
        // contents; there is no immutable element-setter to suggest.
        let src = r#"
            const pdfDocEncodingToUnicode = new Uint16Array(256);
            for (let idx = 0; idx < 256; idx++) {
                pdfDocEncodingToUnicode[idx] = idx;
            }
            pdfDocEncodingToUnicode[0x16] = toCharCode('^W');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typed_array_element_compound_and_update() {
        // Compound assignment (`buf[i] += v`) and update (`buf[i]++`) on a
        // TypedArray element are the same in-place buffer write.
        let src = r#"
            const buf = new Float64Array(8);
            buf[0] += 1.5;
            buf[1]++;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typed_array_element_assignment_via_type_annotation() {
        // A `: Uint8Array` type annotation is the same TypedArray signal even
        // when the initializer is an opaque call.
        let src = r#"
            const buf: Uint8Array = getBuffer();
            buf[0] = 255;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_plain_array_element_assignment() {
        // Negative space: a plain `Array` element write has immutable
        // alternatives (spread, map) — it stays flagged.
        let src = r#"
            const arr = new Array(3);
            arr[0] = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_typed_array_length_property_write() {
        // Negative space: only indexed element writes are exempt; a static
        // property write on a TypedArray (`buf.foo = x`) stays flagged.
        let src = r#"
            const buf = new Uint8Array(4);
            buf.foo = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sparse dispatch-table construction — issue #5412

    #[test]
    fn allows_sparse_dispatch_table_construction_issue_5412() {
        // Regression for rbaumier/comply#5412 — y-websocket message handlers: a
        // locally-owned `const handlers = []` populated by constant-keyed indexed
        // assignment to build an O(1) protocol dispatch table. The sparse layout
        // can't be a constructor literal, so indexed assignment is construction,
        // not mutation.
        let src = r#"
            const messageSync = 0
            const messageAwareness = 1
            const messageHandlers = []
            messageHandlers[messageSync] = (encoder, decoder) => {}
            messageHandlers[messageAwareness] = (encoder, decoder) => {}
            messageHandlers[0x02] = (encoder, decoder) => {}
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_dynamic_index_write_on_local_empty_array_5412() {
        // Negative space: a dynamic (non-constant) index is not the dispatch-table
        // signature — `arr[i] = v` with a `let` loop variable stays flagged.
        let src = r#"
            const arr = [];
            for (let i = 0; i < 3; i++) {
                arr[i] = i;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_object_property_mutation_alongside_dispatch_table_5412() {
        // Negative space: the dispatch-table exemption must not leak — a property
        // write on a const referencing external state stays flagged alongside it.
        let src = r#"
            const handlers = [];
            handlers[0] = fn;
            const cfg = getConfig();
            cfg.x = 2;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Redux Toolkit Immer draft mutations — issue #5596

    #[test]
    fn allows_draft_array_push_in_create_slice_reducer_issue_5596() {
        // A mutating array method (`state.ids.push(…)`) on the Immer draft inside
        // a createSlice reducer is the documented RTK pattern, not aliased state.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const slice = createSlice({
                name: 'entities',
                initialState,
                reducers: {
                    addOne(state, action) {
                        state.ids.push(action.payload.id);
                    },
                },
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_array_push_in_create_reducer_add_case_issue_5596() {
        // Same draft array mutation through `builder.addCase`'s case reducer.
        let src = r#"
            import { createReducer } from '@reduxjs/toolkit'
            const reducer = createReducer(initialState, (builder) => {
                builder.addCase(addTodo, (state, action) => {
                    state.todos.push(action.payload);
                });
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_typed_array_push_issue_5596() {
        // A `Draft<…>`-typed state parameter mutated via a helper — the entity
        // adapter shape; the `Draft` annotation is the structural signal.
        let src = r#"
            import type { Draft } from 'immer';
            function addOneMutably(entity, state: Draft<R>) {
                state.ids.push(entity.id);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_ordinary_const_array_push_outside_reducer_issue_5596() {
        // Negative space: `.push` on a plain const array outside any reducer (no
        // RTK context, no `Draft<…>` type) stays flagged.
        let src = r#"
            function f() {
                const list = getList();
                list.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_immer_draft_typed_const_push_issue_5596() {
        // Negative space: a `Draft<…>` annotation not imported from `immer` is a
        // same-named domain type — `.push` on it stays flagged.
        let src = r#"
            type Draft<T> = T;
            function f() {
                const doc: Draft<Doc> = getDoc();
                doc.items.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_draft_const_mutation_inside_reducer_issue_5596() {
        // Negative space: a captured outer `const` mutated inside a reducer is not
        // the draft (not the reducer's first param) — it stays flagged.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const cache = getCache();
            const slice = createSlice({
                name: 's',
                initialState,
                reducers: {
                    update(state, action) {
                        cache.push(action.payload);
                    },
                },
            })
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // valtio proxy() reactive mutations — issue #5595

    #[test]
    fn allows_valtio_proxy_mutations_issue_5595() {
        // Regression for rbaumier/comply#5595 — valtio's `proxy()` returns a
        // reactive Proxy whose direct mutation IS the API: property assignment,
        // deep update, and mutating array methods on a `const` proxy binding all
        // drive reactivity, with no immutable alternative.
        let src = r#"
            import { proxy } from 'valtio'
            const state = proxy({ number: 0, nested: { ticks: 0 }, items: [] })
            state.number = 1
            state.nested.ticks++
            state.items.push(1)
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_plain_const_mutation_not_valtio_proxy() {
        // Negative space: a plain `const` object (not initialised by `proxy()`
        // from valtio) is not a reactive proxy — mutating it stays flagged, even
        // in a file that imports `proxy` from valtio.
        let src = r#"
            import { proxy } from 'valtio'
            const state = proxy({ n: 0 });
            const plain = getConfig();
            plain.n = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_local_proxy_not_imported_from_valtio() {
        // Negative space: a same-named local `proxy()` (not imported from valtio)
        // returns a plain object — mutating its property stays flagged.
        let src = r#"
            function proxy(x) { return x; }
            const state = proxy({ n: 0 });
            state.n = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Locally-owned fresh-array accumulator, conditional push outside a loop —
    // issue #7593

    #[test]
    fn ignores_conditional_push_on_local_array_literal_issue_7593() {
        // Regression for rbaumier/comply#7593 — documenso search filters: a fresh
        // function-scope array literal built up with a conditional push, then
        // consumed locally (Prisma `where`). Not observable outside the function.
        let src = r#"
            function searchDocuments(user, teamIds, query) {
                const filters = [
                    { recipients: { some: { email: user.email } }, title: { contains: query } },
                ];
                if (teamIds.length > 0) {
                    filters.push({ teamId: { in: teamIds } });
                }
                return prisma.document.findMany({ where: { OR: filters } });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_conditional_push_on_typed_empty_local_array_issue_7593() {
        // Regression for rbaumier/comply#7593 — documenso audit logs: a typed
        // empty local array (`const auditLogs: T[] = []`) accumulated via
        // conditional pushes, then returned.
        let src = r#"
            type AuditLog = { type: string };
            function updateEnvelope(isTitleSame, isExternalIdSame) {
                const auditLogs: AuditLog[] = [];
                if (!isTitleSame) {
                    auditLogs.push({ type: "title" });
                }
                if (!isExternalIdSame) {
                    auditLogs.push({ type: "externalId" });
                }
                return auditLogs;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_top_of_function_push_on_local_array_issue_7593() {
        // Regression for rbaumier/comply#7593 — documenso breadcrumbs: a fresh
        // local array pushed at the top of the function body (no loop). The
        // receiver is a locally-owned identifier, so it is exempt.
        let src = r#"
            function getFolderBreadcrumbs(currentFolder) {
                const breadcrumbs = [];
                breadcrumbs.push(currentFolder);
                return breadcrumbs;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_push_on_const_from_opaque_call_issue_7593() {
        // Negative space: a `const` initialised from a call (not an array literal)
        // may reference a shared array — its `.push` stays flagged.
        let src = r#"
            function collect() {
                const list = getList();
                list.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_module_scope_const_array_issue_7593() {
        // Negative space: a module-scope array is reachable by other code in the
        // module, so its mutation stays observable and flagged.
        let src = r#"
            const registry: number[] = [];
            export function register(x: number) {
                registry.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_member_property_array_issue_7593() {
        // Negative space: `store.items.push(x)` mutates shared object state — the
        // receiver is a member access, not a plain local identifier, so the
        // locally-owned exemption does not apply.
        let src = r#"
            function add(x) {
                const store = getStore();
                store.items.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}

fn report(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span_start: u32, root: &str, kind: &str) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "{kind} `{root}` (declared with `const`) — build a new value instead of mutating."
        ),
        severity: Severity::Warning,
        span: None,
    });
}
