use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::WhileStatement, AstType::DoWhileStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (loop_type, span) = match node.kind() {
            AstKind::WhileStatement(stmt) => ("while", stmt.span),
            AstKind::DoWhileStatement(stmt) => ("do-while", stmt.span),
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-while-loop".into(),
            message: format!("`{loop_type}` loop — prefer recursion or higher-order functions."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(code, &Check)
    }

    #[test]
    fn flags_while() {
        assert_eq!(run("while (true) { break; }").len(), 1);
    }

    #[test]
    fn flags_do_while() {
        assert_eq!(run("do { x++; } while (x < 10);").len(), 1);
    }

    #[test]
    fn allows_for_of() {
        assert!(run("for (const x of items) { process(x); }").is_empty());
    }

    #[test]
    fn allows_map() {
        assert!(run("items.map(x => x * 2);").is_empty());
    }
}
