//! ts-no-unused-vars backend — accurate unused-symbol detection via
//! oxc_semantic.
//!
//! Walks every symbol the semantic builder produced and flags those with
//! no resolved references. Skips:
//! - names starting with `_` (intentional non-use convention)
//! - symbols whose declaration sits inside an `export` (named, default, or
//!   `export *`) — they're part of the public surface
//!
//! Picks up cases the previous text-based heuristic missed: destructuring
//! patterns (`const { x, y } = obj` where `y` is unused), function
//! parameters that share a name with another identifier in the file (the
//! text scan would over-count and mark them used), unused imports, unused
//! type aliases, and shadowed inner bindings that never get referenced.

use oxc_ast::AstKind;

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
                let name = scoping.symbol_name(symbol_id);
                if name.starts_with('_') || name.is_empty() {
                    continue;
                }
                if scoping.get_resolved_references(symbol_id).next().is_some() {
                    continue;
                }

                let decl_node = scoping.symbol_declaration(symbol_id);
                let exported = nodes.ancestor_kinds(decl_node).any(|k| {
                    matches!(
                        k,
                        AstKind::ExportNamedDeclaration(_)
                            | AstKind::ExportDefaultDeclaration(_)
                            | AstKind::ExportAllDeclaration(_)
                    )
                });
                if exported {
                    continue;
                }

                let span = scoping.symbol_span(symbol_id);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line,
                    column,
                    rule_id: "ts-no-unused-vars".into(),
                    message: format!("`{name}` is declared but never used."),
                    severity: Severity::Warning,
                    span: None,
                });
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
    fn flags_unused_variable() {
        let d = run_on("const unused = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`unused`"));
    }

    #[test]
    fn allows_used_variable() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }

    #[test]
    fn allows_underscore_prefix() {
        assert!(run_on("const _unused = 42;").is_empty());
    }

    #[test]
    fn allows_exported_variable() {
        assert!(run_on("export const foo = 42;").is_empty());
    }

    #[test]
    fn flags_multiple_unused() {
        let d = run_on("const aaa = 1; const bbb = 2;");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn flags_unused_destructured_binding() {
        let d = run_on("const obj = { a: 1, b: 2 }; const { a, b } = obj; console.log(a);");
        assert_eq!(d.len(), 1, "destructured `b` is unused");
        assert!(d[0].message.contains("`b`"));
    }

    #[test]
    fn allows_shared_name_with_outer_use() {
        let d = run_on(
            "const x = 1; function f(x: number) { return x; } f(2); console.log(x);",
        );
        assert!(d.is_empty(), "param `x` is used in body, outer `x` is logged");
    }

    #[test]
    fn flags_unused_import() {
        let d = run_on("import { foo } from './x'; console.log('hello');");
        assert_eq!(d.len(), 1, "imported `foo` is never used");
        assert!(d[0].message.contains("`foo`"));
    }
}
