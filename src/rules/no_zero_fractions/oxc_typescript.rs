//! no-zero-fractions oxc backend — flag `1.0`, `2.00`, `3.` number literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NumericLiteral(lit) = node.kind() else { return };

        let text = &ctx.source[lit.span.start as usize..lit.span.end as usize];

        // Must contain a dot to be a decimal literal.
        let Some(dot_pos) = text.find('.') else { return };

        // Skip range operator `..` (shouldn't appear in a number node, but guard).
        if text.get(dot_pos + 1..dot_pos + 2) == Some(".") {
            return;
        }

        let fraction = &text[dot_pos + 1..];

        // Dangling dot: `1.` — fraction is empty.
        let is_dangling = fraction.is_empty();

        // Zero fraction: `1.0`, `1.00`, `1.0_0` — fraction is all zeros/underscores.
        let is_zero_fraction =
            !is_dangling && fraction.chars().all(|c| c == '0' || c == '_');

        if !is_dangling && !is_zero_fraction {
            return;
        }

        let msg = if is_dangling {
            "Don't use a dangling dot in the number."
        } else {
            "Don't use a zero fraction in the number."
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
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
    fn flags_zero_fraction() {
        let d = run_on("const x = 1.0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("zero fraction"));
    }


    #[test]
    fn flags_multiple_trailing_zeros() {
        assert_eq!(run_on("const x = 1.00;").len(), 1);
    }


    #[test]
    fn allows_real_fraction() {
        assert!(run_on("const x = 1.5;").is_empty());
    }


    #[test]
    fn allows_integer() {
        assert!(run_on("const x = 1;").is_empty());
    }


    #[test]
    fn allows_non_zero_fraction() {
        assert!(run_on("const x = 3.14;").is_empty());
    }
}
