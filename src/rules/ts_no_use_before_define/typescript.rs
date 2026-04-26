//! ts-no-use-before-define backend — accurate TDZ detection via
//! oxc_semantic.
//!
//! Walks every block-scoped symbol (`let`, `const`, `class`, `enum`) and
//! checks whether any of its resolved references appears at a source
//! position before the declaration. Skips function declarations and
//! `var` bindings — both are hoisted and not subject to the Temporal
//! Dead Zone.
//!
//! Picks up cases the previous tree-sitter walker missed:
//! - references to a TDZ binding from inside a nested arrow / function
//!   (the heuristic stopped recursing at scope boundaries),
//! - destructuring-pattern declarations,
//! - class declarations referenced before their definition,
//! - block-scoped enums in TS.

use oxc_semantic::SymbolFlags;
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
}
