//! ts-no-use-before-define oxc backend — accurate TDZ detection via
//! oxc_semantic scope/symbol analysis.
//!
//! Skips forward references to bindings initialized via TanStack Router's
//! `createFileRoute(...)` / `createLazyFileRoute(...)` factories. The
//! generated `Route` object is referenced (e.g. `Route.useSearch()`) inside
//! component functions declared above the `export const Route = ...` line;
//! TanStack initializes `Route` before the component renders, so the
//! forward reference is safe.

use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::{NodeId, SymbolFlags};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let flags = scoping.symbol_flags(symbol_id);
            if !flags.intersects(SymbolFlags::BlockScoped) {
                continue;
            }

            let decl_node_id = scoping.symbol_declaration(symbol_id);
            if is_tanstack_route_factory(nodes, decl_node_id) {
                continue;
            }

            let decl_span = scoping.symbol_span(symbol_id);
            let name = scoping.symbol_name(symbol_id);

            for reference in scoping.get_resolved_references(symbol_id) {
                let ref_node_id = reference.node_id();
                let ref_span = nodes.kind(ref_node_id).span();
                if ref_span.start < decl_span.start {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ref_span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("`{name}` is used before its definition."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

/// True when the declarator's initializer is a call to `createFileRoute(...)`
/// or `createLazyFileRoute(...)` — including the curried form
/// `createLazyFileRoute("/users")({ component })`. TanStack Router materializes
/// the `Route` export before any component using `Route.useSearch()` runs, so
/// the forward reference is not a real TDZ hazard.
fn is_tanstack_route_factory<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> bool {
    let iter = std::iter::once(nodes.kind(start)).chain(nodes.ancestor_kinds(start));
    for kind in iter {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl
                .init
                .as_ref()
                .is_some_and(initializer_is_tanstack_route);
        }
    }
    false
}

fn initializer_is_tanstack_route(expr: &Expression) -> bool {
    let Expression::CallExpression(outer) = expr else {
        return false;
    };
    if callee_name(&outer.callee).is_some_and(is_tanstack_route_callee) {
        return true;
    }
    // Curried form: createLazyFileRoute("/users")({ component })
    if let Expression::CallExpression(inner) = &outer.callee
        && callee_name(&inner.callee).is_some_and(is_tanstack_route_callee)
    {
        return true;
    }
    false
}

fn callee_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn is_tanstack_route_callee(name: &str) -> bool {
    matches!(name, "createFileRoute" | "createLazyFileRoute")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_use_before_define() {
        let d = run_on("console.log(x); const x = 1;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_use_after_define() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }

    #[test]
    fn allows_function_declaration_hoisting() {
        assert!(run_on("f(); function f() {}").is_empty());
    }

    #[test]
    fn flags_class_used_before_define() {
        let d = run_on("const c = new C(); class C {}");
        assert_eq!(d.len(), 1, "classes are not hoisted, TDZ applies");
        assert!(d[0].message.contains("`C`"));
    }

    #[test]
    fn flags_use_before_define_from_nested_scope() {
        let d = run_on("const f = () => x; f(); let x = 1;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_var_hoisting() {
        assert!(run_on("console.log(x); var x = 1;").is_empty());
    }

    #[test]
    fn allows_forward_ref_to_tanstack_create_lazy_file_route() {
        // TanStack Router lazy-route pattern: the component references
        // `Route.useSearch()` before `export const Route = createLazyFileRoute(...)`.
        let source = "function UsersPage() {\n\
                      const search = Route.useSearch();\n\
                      return null;\n\
                      }\n\
                      export const Route = createLazyFileRoute(\"/users\")({ component: UsersPage });";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_forward_ref_to_tanstack_create_file_route() {
        let source = "function UsersPage() {\n\
                      const nav = Route.useNavigate();\n\
                      return null;\n\
                      }\n\
                      export const Route = createFileRoute(\"/users\")({ component: UsersPage });";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_non_tanstack_forward_ref() {
        let d = run_on(
            "function f() { return Route.x; }\n\
             const Route = makeRoute();",
        );
        assert_eq!(d.len(), 1, "non-TanStack forward refs still flagged");
    }
}
