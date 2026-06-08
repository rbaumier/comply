//! sql-no-now-in-transaction — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        let upper = text.to_ascii_uppercase();
        if !(upper.contains("BEGIN") || upper.contains("START TRANSACTION")) {
            return;
        }
        if !(upper.contains("NOW()") || upper.contains("CURRENT_TIMESTAMP")) {
            return;
        }
        if !super::sql_uses_now_in_tx(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`NOW()`/`CURRENT_TIMESTAMP` freezes at transaction start — use `clock_timestamp()` inside BEGIN blocks.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_now_in_begin_block() {
        let src = "const q = `BEGIN;\nINSERT INTO log (ts) VALUES (NOW());\nCOMMIT;`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_now_outside_tx() {
        let src = "const q = `INSERT INTO log (ts) VALUES (NOW());`;";
        assert!(run_on(src).is_empty());
    }



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn allows_clock_timestamp_in_tx() {
        let src = "const q = `BEGIN;\nINSERT INTO log (ts) VALUES (clock_timestamp());\nCOMMIT;`;";
        assert!(run(src).is_empty());
    }
}
