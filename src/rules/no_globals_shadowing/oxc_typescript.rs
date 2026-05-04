use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
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
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Local variable shadows global `{name}` — rename to avoid confusion."
                ),
                severity: super::META.severity,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
        assert_eq!(run_on("const { console } = obj;").len(), 1);
    }

    #[test]
    fn flags_function_param_console() {
        assert_eq!(
            run_on("function f(console: any) { return console; }").len(),
            1
        );
    }
}
