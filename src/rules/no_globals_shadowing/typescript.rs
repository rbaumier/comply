//! no-globals-shadowing — TS / JS / TSX backend.
//!
//! Walks every symbol in the program and flags any whose name matches
//! a well-known global (`console`, `window`, `process`, …). Catching
//! the symbol rather than the AST node form covers destructured names
//! (`const { console } = obj`), function parameters (`function f(console)
//! {}`), import bindings, class names, and TS namespaces — none of
//! which the previous AST walker (variable_declarator only) flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

const SHADOWED_GLOBALS: &[&str] = &[
    "console",
    "window",
    "document",
    "process",
    "global",
    "globalThis",
    "setTimeout",
    "setInterval",
];

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let mut diagnostics = Vec::new();

            for symbol_id in scoping.symbol_ids() {
                let name = scoping.symbol_name(symbol_id);
                if !SHADOWED_GLOBALS.contains(&name) {
                    continue;
                }
                let span = scoping.symbol_span(symbol_id);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-globals-shadowing".into(),
                    message: format!(
                        "Local variable shadows global `{name}` — rename to avoid confusion."
                    ),
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
    fn flags_const_console() {
        assert_eq!(run_on("const console = {};").len(), 1);
    }

    #[test]
    fn flags_let_window() {
        assert_eq!(run_on("let window = {};").len(), 1);
    }

    #[test]
    fn allows_different_name() {
        assert!(run_on("const myConsole = {};").is_empty());
    }

    #[test]
    fn allows_console_usage() {
        assert!(run_on("console.log('hello');").is_empty());
    }

    #[test]
    fn flags_destructured_console() {
        // `const { console } = obj` was missed by the previous walker
        // because the binding is a shorthand_property_identifier, not a
        // plain identifier child of variable_declarator.
        assert_eq!(run_on("const { console } = obj;").len(), 1);
    }

    #[test]
    fn flags_function_param_console() {
        // Same story for params.
        assert_eq!(
            run_on("function f(console: any) { return console; }").len(),
            1
        );
    }
}
