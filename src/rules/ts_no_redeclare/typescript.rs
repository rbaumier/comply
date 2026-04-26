//! ts-no-redeclare backend — detect duplicate variable declarations in
//! the same scope, via oxc_semantic.
//!
//! Walks every symbol and reports each entry returned by
//! `symbol_redeclarations` (oxc tracks duplicate `var`, `function` and
//! TS declaration-merging compatible redeclarations on the original
//! symbol).
//!
//! The previous implementation rebuilt scope identity from
//! tree-sitter parent kinds, which only supported `function` /
//! `function_declaration` / `arrow_function` / `method_definition` /
//! `statement_block` and treated for-headers, catch clauses, switch
//! blocks, and class bodies as the surrounding scope — leading to both
//! false positives (declarations in for-init reused in the loop body)
//! and false negatives (`for (let x of …)` followed by another `let x`
//! in the same block was missed when the for-statement was nested).
//! Using oxc's symbol model removes that whole class of bugs.

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
                // `symbol_declarations` returns every declaration node bound to
                // a symbol; the first is the original, anything after it is a
                // redeclaration we should flag.
                let mut iter = scoping.symbol_declarations(symbol_id);
                if iter.next().is_none() {
                    continue;
                }
                let name = scoping.symbol_name(symbol_id);
                for decl_id in iter {
                    let span = nodes.kind(decl_id).span();
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "ts-no-redeclare".into(),
                        message: format!("`{name}` is already defined."),
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
    fn flags_duplicate_var() {
        let d = run_on("var x = 1; var x = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_different_scopes() {
        let d = run_on("function a() { let x = 1; } function b() { let x = 2; }");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_duplicate_function_declaration() {
        // Two `function foo` at the same scope is a redeclaration that
        // the previous walker missed (it only inspected
        // variable_declarator nodes).
        let d = run_on("function foo() {} function foo() {}");
        assert_eq!(d.len(), 1);
    }
}
