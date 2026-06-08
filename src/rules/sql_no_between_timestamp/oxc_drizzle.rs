use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
        if id.name.as_str() != "between" {
            return;
        }
        let Some(first) = call.arguments.first() else {
            return;
        };
        let prop_name = match first.as_expression() {
            Some(Expression::StaticMemberExpression(m)) => m.property.name.as_str(),
            _ => return,
        };
        if !looks_like_timestamp_column(prop_name) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`between()` on a timestamp column has an off-by-one — \
                      use `gte(col, start)` and `lt(col, end)` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn looks_like_timestamp_column(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with("_at")
        || lower.ends_with("at") && has_camel_at_suffix(name)
        || lower.contains("time")
        || lower.contains("date")
        || lower.contains("timestamp")
        || lower.contains("created")
        || lower.contains("updated")
        || lower.contains("deleted")
        || lower.contains("expired")
}

fn has_camel_at_suffix(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    let n = bytes.len();
    bytes[n - 2] == b'A' && bytes[n - 1] == b't'
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
    fn flags_between_on_camel_created_at() {
        assert_eq!(run_on("where(between(users.createdAt, start, end));").len(), 1);
    }

    #[test]
    fn flags_between_on_snake_updated_at() {
        assert_eq!(run_on("where(between(orders.updated_at, d1, d2));").len(), 1);
    }

    #[test]
    fn allows_between_on_price() {
        assert!(run_on("where(between(products.price, 10, 100));").is_empty());
    }
}
