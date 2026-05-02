//! Vue SFC oxc integration — parse `<script>` blocks with oxc_parser.
//!
//! Reuses `vue_sfc::extract_scripts` for block extraction (tree-sitter-vue),
//! then parses each block's text body with oxc_parser and runs `OxcCheck`
//! rules against the resulting `Semantic`.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::vue_sfc::ScriptBlock;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

/// Parse a Vue `<script>` block with oxc and run a single `OxcCheck` rule
/// against it. Translates diagnostic coordinates from the inner block back
/// to the outer Vue file.
pub fn run_oxc_check_on_vue_block(
    block: &ScriptBlock<'_>,
    check: &dyn OxcCheck,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if block.text.trim().is_empty() {
        return;
    }

    // tsx is the most permissive source type — accepts TS, JS, and JSX.
    // ScriptBlock has no `is_typescript` field; using tsx unconditionally.
    let source_type = SourceType::tsx();
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, block.text, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;

    let mut inner_diags = Vec::new();
    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        inner_diags = check.run_on_semantic(&semantic, ctx);
    } else {
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty();
            if kinds.contains(&ty) {
                check.run(node, ctx, &semantic, &mut inner_diags);
            }
        }
    }

    // Translate coordinates from inner block back to Vue file.
    for d in &mut inner_diags {
        d.line += block.start_row;
        if d.line == block.start_row + 1 {
            // First line of the block: column is relative to block start.
            d.column += block.start_column;
        }
    }

    diagnostics.extend(inner_diags);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::AstType;
    use oxc_semantic::AstNode;
    use std::path::Path;
    use std::sync::Arc;

    /// Dummy rule that flags every `CatchClause` it sees.
    struct FlagCatch;

    impl OxcCheck for FlagCatch {
        fn interested_kinds(&self) -> &'static [AstType] {
            &[AstType::CatchClause]
        }

        fn run<'a>(
            &self,
            node: &AstNode<'a>,
            ctx: &CheckCtx,
            semantic: &'a oxc_semantic::Semantic<'a>,
            diagnostics: &mut Vec<Diagnostic>,
        ) {
            let span = match node.kind() {
                oxc_ast::AstKind::CatchClause(c) => c.span,
                _ => return,
            };
            let line = semantic.source_text()[..span.start as usize]
                .lines()
                .count();
            diagnostics.push(Diagnostic {
                path: ctx.path.into(),
                line,
                column: 0,
                rule_id: "test-flag-catch".into(),
                message: "found catch".into(),
                severity: crate::diagnostic::Severity::Warning,
                span: None,
            });
        }
    }

    #[test]
    fn translates_line_offset() {
        // Simulate a block starting at row 2 (0-indexed) in the Vue file,
        // e.g. after `<template>...</template>\n<script>\n`.
        let block = ScriptBlock {
            text: "try { } catch (e) { }",
            start_row: 2,
            start_column: 0,
            is_setup: false,
        };
        let ctx = CheckCtx::for_test(Path::new("test.vue"), block.text);
        let mut diags = Vec::new();
        run_oxc_check_on_vue_block(&block, &FlagCatch, &ctx, &mut diags);
        assert_eq!(diags.len(), 1);
        // Inner line 1 + start_row 2 = 3.
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn empty_block_produces_no_diagnostics() {
        let block = ScriptBlock {
            text: "  \n  ",
            start_row: 1,
            start_column: 0,
            is_setup: true,
        };
        let ctx = CheckCtx::for_test(Path::new("test.vue"), block.text);
        let mut diags = Vec::new();
        run_oxc_check_on_vue_block(&block, &FlagCatch, &ctx, &mut diags);
        assert!(diags.is_empty());
    }

    /// Dummy rule using `run_on_semantic` (no interested_kinds).
    struct CountDeclarations;

    impl OxcCheck for CountDeclarations {
        fn run_on_semantic<'a>(
            &self,
            semantic: &'a oxc_semantic::Semantic<'a>,
            ctx: &CheckCtx,
        ) -> Vec<Diagnostic> {
            let count = semantic
                .nodes()
                .iter()
                .filter(|n| matches!(n.kind(), oxc_ast::AstKind::VariableDeclarator(_)))
                .count();
            if count > 0 {
                vec![Diagnostic {
                    path: ctx.path.into(),
                    line: 1,
                    column: 0,
                    rule_id: "test-count-decls".into(),
                    message: format!("{count} declarations"),
                    severity: crate::diagnostic::Severity::Warning,
                    span: None,
                }]
            } else {
                Vec::new()
            }
        }
    }

    #[test]
    fn run_on_semantic_path_works() {
        let block = ScriptBlock {
            text: "const a = 1;\nlet b = 2;",
            start_row: 5,
            start_column: 0,
            is_setup: false,
        };
        let ctx = CheckCtx::for_test(Path::new("app.vue"), block.text);
        let mut diags = Vec::new();
        run_oxc_check_on_vue_block(&block, &CountDeclarations, &ctx, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("2 declarations"));
        // Line 1 + start_row 5 = 6.
        assert_eq!(diags[0].line, 6);
    }
}
