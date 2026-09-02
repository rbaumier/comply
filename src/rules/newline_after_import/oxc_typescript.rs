//! newline-after-import OXC backend — flag the last import when the next
//! top-level statement follows on the immediately next line with no blank line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let body = &semantic.nodes().program().body;

        // Find the index of the last ImportDeclaration.
        let last_import_idx = body
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| matches!(s, Statement::ImportDeclaration(_)))
            .map(|(i, _)| i);

        let Some(last_import_idx) = last_import_idx else {
            return Vec::new();
        };

        let import_stmt = &body[last_import_idx];

        // Find next non-empty statement after the last import.
        let next_stmt = body
            .iter()
            .skip(last_import_idx + 1)
            .find(|s| !matches!(s, Statement::EmptyStatement(_)));

        let Some(next_stmt) = next_stmt else {
            return Vec::new();
        };

        let import_span = match import_stmt {
            Statement::ImportDeclaration(d) => d.span,
            _ => return Vec::new(),
        };
        let next_span = match next_stmt {
            Statement::ExpressionStatement(s) => s.span,
            Statement::BlockStatement(s) => s.span,
            Statement::VariableDeclaration(s) => s.span,
            Statement::FunctionDeclaration(s) => oxc_span::GetSpan::span(s.as_ref()),
            Statement::ClassDeclaration(s) => oxc_span::GetSpan::span(s.as_ref()),
            Statement::ExportNamedDeclaration(s) => s.span,
            Statement::ExportDefaultDeclaration(s) => s.span,
            Statement::ExportAllDeclaration(s) => s.span,
            Statement::IfStatement(s) => s.span,
            Statement::ForStatement(s) => s.span,
            Statement::WhileStatement(s) => s.span,
            Statement::ReturnStatement(s) => s.span,
            Statement::TryStatement(s) => s.span,
            Statement::SwitchStatement(s) => s.span,
            Statement::ThrowStatement(s) => s.span,
            _ => return Vec::new(),
        };

        let (import_end_line, _) = byte_offset_to_line_col(ctx.source, import_span.end as usize);
        let (next_start_line, _) = byte_offset_to_line_col(ctx.source, next_span.start as usize);

        // If the next statement starts on the line right after the import ends,
        // there is no blank line separator.
        if next_start_line == import_end_line + 1 {
            // Anchor on the import declaration itself, so the caret lands on the
            // `import` keyword rather than on column 1 of that line.
            return vec![Diagnostic::at_offset(
                Arc::clone(&ctx.path_arc),
                ctx.source,
                (import_span.start as usize, import_span.size() as usize),
                super::META.id,
                "Expected a blank line after the last import statement.".into(),
                Severity::Error,
            )];
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id(super::super::META.id, source, "t.ts")
    }

    #[test]
    fn anchors_on_the_import_declaration() {
        // Regression for rbaumier/comply#8386 — the rule held the import's span
        // and emitted `column: 1, span: None`, so nothing underlined the
        // statement the message names.
        let src = "import a from 'a';\nconst b = 1;\n";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (1, 1));
        let (offset, len) = diags[0].span.expect("the anchor carries the import's span");
        assert_eq!(&src[offset..offset + len], "import a from 'a';");
    }
}
