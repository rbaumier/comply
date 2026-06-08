//! pure-by-default OXC backend.
//!
//! Pub/sub store exception: if the module declares a root-scope `const`
//! variable initialised with `new Set(...)` (the subscriber list), the entire
//! file is treated as an intentional store and no violations are emitted.
//! This recognises the `useSyncExternalStore` / observer idiom where
//! module-level mutable state is the explicit architectural contract.

use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, VariableDeclarationKind};
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
            let var_name = scoping.symbol_name(symbol_id).to_string();

            for reference in scoping.get_resolved_references(symbol_id) {
                let Some((func_id, func_name)) =
                    enclosing_top_level_function(nodes, reference.node_id())
                else {
                    continue;
                };
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
}
