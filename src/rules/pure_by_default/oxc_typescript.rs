//! pure-by-default OXC backend.
//!
//! Pub/sub store exception: if the module declares a root-scope `const`
//! variable initialised with `new Set(...)` (the subscriber list), the entire
//! file is treated as an intentional store and no violations are emitted.
//! This recognises the `useSyncExternalStore` / observer idiom where
//! module-level mutable state is the explicit architectural contract.

use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, Expression, Statement,
    VariableDeclarationKind,
};
use oxc_semantic::NodeId;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let root_scope = scoping.root_scope_id();

        if has_root_scope_const_set(nodes, scoping, root_scope) {
            return vec![];
        }

        let mut diagnostics = Vec::new();
        let mut flagged: HashSet<NodeId> = HashSet::new();

        for symbol_id in scoping.symbol_ids() {
            if scoping.symbol_scope_id(symbol_id) != root_scope {
                continue;
            }
            if !is_let_or_var(nodes, scoping.symbol_declaration(symbol_id)) {
                continue;
            }
            if is_effectively_const_binding(scoping, symbol_id) {
                continue;
            }
            let var_name = scoping.symbol_name(symbol_id).to_string();

            for reference in scoping.get_resolved_references(symbol_id) {
                let Some((func_id, func_name)) =
                    enclosing_top_level_function(nodes, reference.node_id())
                else {
                    continue;
                };
                if is_pure_setter_for(nodes, func_id, func_name, &var_name) {
                    continue;
                }
                if !flagged.insert(func_id) {
                    continue;
                }
                let func_span = nodes.kind(func_id).span();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, func_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{func_name}` references mutable top-level state `{var_name}`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// Returns true when the module contains a root-scope `const` variable
/// initialised with `new Set(...)`, which signals an intentional pub/sub or
/// observer store pattern.
fn has_root_scope_const_set(
    nodes: &oxc_semantic::AstNodes,
    scoping: &oxc_semantic::Scoping,
    root_scope: oxc_semantic::ScopeId,
) -> bool {
    scoping.symbol_ids().any(|symbol_id| {
        scoping.symbol_scope_id(symbol_id) == root_scope
            && is_const_new_set(nodes, scoping.symbol_declaration(symbol_id))
    })
}

/// True if the symbol's declaration is `const x = new Set(...)`.
fn is_const_new_set(nodes: &oxc_semantic::AstNodes, decl_id: NodeId) -> bool {
    // `ancestor_kinds` does not include the node itself, so prepend it.
    let mut init_is_set = false;
    for kind in
        std::iter::once(nodes.kind(decl_id)).chain(nodes.ancestor_kinds(decl_id))
    {
        match kind {
            AstKind::VariableDeclarator(declarator) => {
                init_is_set = declarator
                    .init
                    .as_ref()
                    .is_some_and(is_new_set_expression);
            }
            AstKind::VariableDeclaration(decl) => {
                return init_is_set
                    && matches!(decl.kind, VariableDeclarationKind::Const);
            }
            _ => {}
        }
    }
    false
}

fn is_new_set_expression(expr: &Expression) -> bool {
    let Expression::NewExpression(new_expr) = expr else {
        return false;
    };
    let Expression::Identifier(ident) = &new_expr.callee else {
        return false;
    };
    ident.name.as_str() == "Set"
}

/// True if a `let`/`var` binding is never reassigned after its declarator, so
/// it is constant in practice. A function reading such a binding is pure with
/// respect to it. `Scoping::symbol_is_mutated` reports whether any resolved
/// reference writes to the symbol; the initial declarator is not a reference,
/// so an alias like `let isArray = Array.isArray` reports `false`.
fn is_effectively_const_binding(
    scoping: &oxc_semantic::Scoping,
    symbol_id: oxc_semantic::SymbolId,
) -> bool {
    !scoping.symbol_is_mutated(symbol_id)
}

/// True if the symbol's declaration sits inside a `let` or `var`
/// `VariableDeclaration`.
fn is_let_or_var(nodes: &oxc_semantic::AstNodes, decl_id: NodeId) -> bool {
    for kind in nodes.ancestor_kinds(decl_id) {
        if let AstKind::VariableDeclaration(decl) = kind {
            return matches!(
                decl.kind,
                VariableDeclarationKind::Let | VariableDeclarationKind::Var
            );
        }
    }
    false
}

/// Walk up from `start` until we hit a `Function` declaration whose
/// nearest enclosing scope is the program. Returns `(node_id, name)`.
fn enclosing_top_level_function<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> Option<(NodeId, &'a str)> {
    let mut last_function: Option<(NodeId, &'a str)> = None;
    for (kind, node_id) in nodes.ancestor_kinds(start).zip(nodes.ancestor_ids(start)) {
        match kind {
            AstKind::Function(func) => {
                if let Some(ident) = &func.id {
                    last_function = Some((node_id, ident.name.as_str()));
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                return None;
            }
            AstKind::Program(_) => {
                return last_function;
            }
            _ => {}
        }
    }
    None
}

/// True when `func_id` is a deliberate public setter for `var_name`: its body
/// does nothing but assign to that module-level binding, and its name follows
/// the setter convention. Such a function is impure by design — mutating the
/// config is its sole stated purpose — so it is not a violation.
///
/// Both signals are required for precision:
/// - structural: the body is a single `var_name = <expr>` assignment, optionally
///   followed by a bare `return;` (void return);
/// - naming: the name starts with `set`, or `var_name` appears within the name.
///
/// A function that also reads the binding for a computation, returns a value, or
/// runs any other statement fails the structural check and stays flagged.
fn is_pure_setter_for(
    nodes: &oxc_semantic::AstNodes,
    func_id: NodeId,
    func_name: &str,
    var_name: &str,
) -> bool {
    let AstKind::Function(func) = nodes.kind(func_id) else {
        return false;
    };
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    if !body_is_sole_assignment_to(&body.statements, var_name) {
        return false;
    }
    name_follows_setter_convention(func_name, var_name)
}

/// True when `statements` is exactly a single assignment `var_name = <expr>`
/// (plain `=`, never a compound op), optionally followed by a bare `return;`.
fn body_is_sole_assignment_to(statements: &[Statement], var_name: &str) -> bool {
    let mut iter = statements.iter();
    let Some(first) = iter.next() else {
        return false;
    };
    if !is_plain_assignment_to(first, var_name) {
        return false;
    }
    match iter.next() {
        None => true,
        Some(second) => is_bare_return(second) && iter.next().is_none(),
    }
}

/// True if `stmt` is `var_name = <expr>;` with the plain `=` operator.
fn is_plain_assignment_to(stmt: &Statement, var_name: &str) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
        return false;
    };
    if assign.operator != AssignmentOperator::Assign {
        return false;
    }
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        return false;
    };
    target.name.as_str() == var_name
}

/// True if `stmt` is `return;` with no returned value.
fn is_bare_return(stmt: &Statement) -> bool {
    matches!(stmt, Statement::ReturnStatement(ret) if ret.argument.is_none())
}

/// True if the function name follows the setter convention for `var_name`:
/// it starts with `set`, or `var_name` appears within it (case-insensitive).
fn name_follows_setter_convention(func_name: &str, var_name: &str) -> bool {
    func_name.starts_with("set")
        || func_name.to_ascii_lowercase().contains(&var_name.to_ascii_lowercase())
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
    fn flags_function_using_top_level_let() {
        let src = "let counter = 0;\nfunction increment() { counter += 1; }\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn allows_function_without_top_level_state() {
        let src = "const MAX = 100;\nfunction add(a: number, b: number) { return a + b; }\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_var_at_top_level() {
        let src = "var state = {};\nfunction reset() { state = {}; }\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reset"));
    }

    // Regression for #577 — pub/sub store with useSyncExternalStore pattern.
    // The module-level `const subscribers = new Set<() => void>()` signals
    // intentional mutable state; no violation should be emitted.
    #[test]
    fn no_fp_on_pubsub_store_with_const_set() {
        let src = r#"
let titleByPathname = new Map<string, string>();
const subscribers = new Set<() => void>();

export function setLiveRouteTitle(pathname: string, title: string) {
    titleByPathname.set(pathname, title);
    subscribers.forEach((cb) => cb());
}

export function clearLiveRouteTitle(pathname: string) {
    titleByPathname.delete(pathname);
    subscribers.forEach((cb) => cb());
}

export function getLiveRouteTitlesSnapshot() {
    return titleByPathname;
}

export function resetLiveRouteTitlesForTests() {
    titleByPathname = new Map();
}
"#;
        assert!(run(src).is_empty(), "pub/sub store functions must not be flagged");
    }

    #[test]
    fn no_fp_on_module_with_exported_const_set_subscriber() {
        // Even when the Set is exported, the pattern is still a pub/sub store.
        let src = r#"
let state = 0;
export const listeners = new Set<() => void>();
export function setState(n: number) { state = n; listeners.forEach(l => l()); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutable_state_without_const_set() {
        // A plain mutable module-level variable without a subscriber Set is
        // still a violation.
        let src = r#"
let counter = 0;
export function increment() { counter += 1; }
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
    }

    // Regression for #1890 — immer-style `export let` aliases that are
    // declared once and never reassigned are effectively constant; a function
    // reading them is pure with respect to that binding.
    #[test]
    fn no_fp_on_never_reassigned_root_let() {
        let src = r#"
export let isArray = Array.isArray
export let isObjectish = (target: any) => typeof target === "object"

export function isDraftable(value: any): boolean {
    if (!value) return false
    return isObjectish(value) || isArray(value)
}
"#;
        assert!(
            run(src).is_empty(),
            "never-reassigned root-scope let must not be flagged"
        );
    }

    #[test]
    fn flags_reassigned_root_let_read_by_function() {
        // A counter that IS reassigned (`counter = ...`) and read inside a
        // function is genuine mutable state and must still be flagged.
        let src = r#"
let counter = 0;
export function bump() {
    counter = counter + 1;
    return counter;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bump"));
        assert!(d[0].message.contains("counter"));
    }

    // Regression for #2241 — a deliberate public config setter whose entire
    // body is a single assignment to the module-level `let` is impure by
    // design, not an accidental side effect.
    #[test]
    fn no_fp_on_public_config_setter() {
        let src = r#"
export let mapStoreSuffix = 'Store'
export function setMapStoreSuffix(suffix: string): void {
    mapStoreSuffix = suffix
}
"#;
        assert!(
            run(src).is_empty(),
            "a pure setter (single module-let assignment) must not be flagged"
        );
    }

    #[test]
    fn no_fp_on_setter_with_trailing_void_return() {
        // A trailing bare `return;` is still a void setter.
        let src = r#"
let theme = 'light'
export function setTheme(next: string): void {
    theme = next
    return
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_setter_named_after_the_variable() {
        // Name does not start with `set` but contains the variable name.
        let src = r#"
let locale = 'en'
export function updateLocale(next: string) {
    locale = next
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_setter_that_does_extra_work() {
        // Body is more than the assignment: it also reads/computes, so it is
        // not a pure setter and stays flagged.
        let src = r#"
let count = 0
export function setCount(n: number) {
    count = n
    console.log(count)
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setCount"));
    }

    #[test]
    fn flags_setter_with_compound_assignment() {
        // A compound `+=` reads the previous value, so it is not a plain
        // setter even with a setter-style name.
        let src = r#"
let total = 0
export function setTotal(n: number) {
    total += n
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setTotal"));
    }

    #[test]
    fn flags_function_that_only_reads_mutable_state() {
        // Reads a mutable top-level `let` for a computation — no assignment at
        // all, so it is not a setter and must stay flagged.
        let src = r#"
let counter = 1
export function setCounter(n: number) {
    counter = n
}
export function compute() {
    return counter * 2
}
"#;
        let d = run(src);
        // `setCounter` is a pure setter (exempt); `compute` reads the mutable
        // state for a computation and must stay flagged.
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("compute"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn flags_single_assignment_without_setter_name() {
        // Body is a single assignment but the name neither starts with `set`
        // nor mentions the variable — the naming signal is missing, so the
        // structural-only match is rejected.
        let src = r#"
let flag = false
export function toggle(v: boolean) {
    flag = v
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toggle"));
    }
}
