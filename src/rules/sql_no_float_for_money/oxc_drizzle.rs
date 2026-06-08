use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const MONEY_KEYWORDS: &[&str] = &[
    "price", "amount", "money", "cost", "fee", "total", "balance", "salary", "revenue", "budget",
    "payment", "rate", "discount", "tax", "charge",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        let name = id.name.as_str();
        if name != "real" && name != "doublePrecision" {
            return;
        }
        for arg in &call.arguments {
            if let Argument::StringLiteral(lit) = arg {
                let col_name = lit.value.as_str();
                let lower = col_name.to_ascii_lowercase();
                if MONEY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
                    let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}('{col_name}')` uses floating-point for a monetary \
                             column — use `numeric(precision, scale)` to avoid \
                             rounding errors that compound over transactions."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                break;
            }
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_real_price() {
        assert_eq!(run_on("const price = real('price');").len(), 1);
    }

    #[test]
    fn flags_double_precision_amount() {
        assert_eq!(run_on("const amount = doublePrecision('total_amount');").len(), 1);
    }

    #[test]
    fn does_not_flag_latitude() {
        assert!(run_on("const lat = doublePrecision('latitude');").is_empty());
    }
}
