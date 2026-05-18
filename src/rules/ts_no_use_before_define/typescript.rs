//! ts-no-use-before-define backend — accurate TDZ detection via
//! oxc_semantic.
//!
//! Walks every block-scoped symbol (`let`, `const`, `class`, `enum`) and
//! checks whether any of its resolved references appears at a source
//! position before the declaration. Skips function declarations and
//! `var` bindings — both are hoisted and not subject to the Temporal
//! Dead Zone.
//!
//! Also skips bindings initialized via TanStack Router's
//! `createFileRoute(...)` / `createLazyFileRoute(...)` factories: the
//! generated `Route` object is referenced (e.g. `Route.useSearch()`)
//! inside component functions declared above the `export const Route = ...`
//! line, and TanStack initializes `Route` before the component renders.

use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::{NodeId, SymbolFlags};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let nodes = semantic.nodes();
            let mut diagnostics = Vec::new();

            for symbol_id in scoping.symbol_ids() {
                let flags = scoping.symbol_flags(symbol_id);
                // Only block-scoped declarations have a Temporal Dead Zone.
                // `var` and function declarations are hoisted.
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
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "ts-no-use-before-define".into(),
                            message: format!("`{name}` is used before its definition."),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }

            diagnostics
        })
    }
}

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

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
        // Reference lives inside a nested arrow but resolves to the
        // outer `let x` declared after the function expression. This
        // is the TDZ-leak the tree-sitter heuristic missed because it
        // stopped recursing at function boundaries.
        let d = run_on("const f = () => x; f(); let x = 1;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_var_hoisting() {
        // `var` is function-scoped and hoisted: not a TDZ violation.
        assert!(run_on("console.log(x); var x = 1;").is_empty());
    }

    #[test]
    fn allows_forward_ref_to_tanstack_create_lazy_file_route() {
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
