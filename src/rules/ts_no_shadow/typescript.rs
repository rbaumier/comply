//! ts-no-shadow backend — accurate variable shadowing detection via
//! oxc_semantic.
//!
//! Walks every symbol in the program: if a symbol's enclosing scope has a
//! parent scope that already binds the same name, it's a shadow. Unlike
//! the previous tree-sitter heuristic, this picks up destructuring
//! patterns, catch parameters, class members, function-expression
//! identifiers, and TS-specific declarations (enum, namespace) that the
//! manual walker silently missed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;
use oxc_ast::AstKind;

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
                let scope_id = scoping.symbol_scope_id(symbol_id);
                let Some(parent_scope) = scoping.scope_parent_id(scope_id) else {
                    continue;
                };
                let name = scoping.symbol_name(symbol_id);
                if is_single_uppercase(name) {
                    continue;
                }
                let decl_node = scoping.symbol_declaration(symbol_id);
                if std::iter::once(nodes.kind(decl_node))
                    .chain(nodes.ancestor_kinds(decl_node))
                    .any(is_type_only_binding_context)
                {
                    continue;
                }
                let ident = oxc_str::Ident::from(name);
                if scoping.find_binding(parent_scope, ident).is_some() {
                    let span = scoping.symbol_span(symbol_id);
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "ts-no-shadow".into(),
                        message: format!("`{name}` is already declared in an outer scope."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }

            diagnostics
        })
    }
}

fn is_single_uppercase(name: &str) -> bool {
    name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase()
}

/// True when the binding is in a type-only context (function/index/mapped/infer)
/// whose names are not accessible at runtime.
fn is_type_only_binding_context(kind: AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::TSConstructSignatureDeclaration(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSIndexSignature(_)
            | AstKind::TSMappedType(_)
            | AstKind::TSInferType(_)
    )
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
    fn flags_shadowed_variable() {
        let d = run_on("const x = 1; function f() { const x = 2; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_different_names() {
        assert!(run_on("const x = 1; function f() { const y = 2; }").is_empty());
    }

    #[test]
    fn flags_param_shadowing_outer() {
        let d = run_on("const x = 1; function f(x: number) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_nested_shadow() {
        let d = run_on("const a = 1; function f() { const a = 2; function g() { const a = 3; } }");
        assert!(d.len() >= 2);
    }

    #[test]
    fn flags_destructuring_shadow() {
        let d = run_on("const x = 1; function f() { const { x } = obj; }");
        assert_eq!(d.len(), 1, "destructured `x` shadows outer `x`");
    }

    #[test]
    fn flags_catch_parameter_shadow() {
        let d = run_on("const e = 1; try { foo(); } catch (e) { console.log(e); }");
        assert_eq!(d.len(), 1, "catch param `e` shadows outer `e`");
    }

    #[test]
    fn allows_single_uppercase_type_param_shadow() {
        let d = run_on("function f<A>(a: A) { function g<A>(b: A) { return b; } return g(a); }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_repeated_param_names_in_nested_function_type_signatures() {
        let d = run_on(
            "type AnyUseNavigate = (...args: never[]) => (...args: never[]) => unknown;",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_in_function_type_alias_matching_outer_const() {
        let d = run_on("const x = 1; type F = (x: number) => void;");
        assert!(d.is_empty(), "param inside function type should not shadow outer `x`");
    }

    #[test]
    fn still_flags_shadowing_in_real_function() {
        // Real function params still flag as shadows.
        let d = run_on("const x = 1; function f(x: number) { return x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_index_signature_parameter_with_shadow() {
        let d = run_on("interface I { [key: string]: number } const key = \"x\";");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_mapped_type_key_with_shadow() {
        let d = run_on("type M<T> = { [K in keyof T]: T[K] }; const K = 1;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_infer_type_parameter_with_shadow() {
        let d = run_on("type Unpack<T> = T extends Promise<infer R> ? R : never; const R = 1;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }
}
