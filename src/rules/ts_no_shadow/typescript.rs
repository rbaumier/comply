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

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let mut diagnostics = Vec::new();

            for symbol_id in scoping.symbol_ids() {
                let scope_id = scoping.symbol_scope_id(symbol_id);
                let Some(parent_scope) = scoping.scope_parent_id(scope_id) else {
                    continue;
                };
                let name = scoping.symbol_name(symbol_id);
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
        let d = run_on(
            "const a = 1; function f() { const a = 2; function g() { const a = 3; } }",
        );
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
}
