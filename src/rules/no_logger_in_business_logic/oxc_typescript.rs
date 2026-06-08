//! no-logger-in-business-logic — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Path fragments that mark a business-logic directory, pre-expanded with
/// both path separators so the per-node check needs no `format!` allocation.
const BUSINESS_DIR_PATTERNS: &[&str] = &[
    "/service/", "\\service\\",
    "/domain/", "\\domain\\",
    "/core/", "\\core\\",
    "/model/", "\\model\\",
    "/entity/", "\\entity\\",
];

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug", "trace"];

fn is_business_logic_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    BUSINESS_DIR_PATTERNS.iter().any(|p| path_str.contains(p))
}

/// Return the leftmost identifier name in a (possibly chained) member expression.
fn root_identifier_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(&id.name),
        Expression::ThisExpression(_) => Some("this"),
        Expression::StaticMemberExpression(mem) => root_identifier_name(&mem.object),
        Expression::ComputedMemberExpression(mem) => root_identifier_name(&mem.object),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_business_logic_path(ctx.path) {
            return;
        }

        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a static member expression (e.g. console.log, logger.info).
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };

        let prop_text = &*mem.property.name;
        let Some(root) = root_identifier_name(&mem.object) else {
            return;
        };

        let pattern = match root {
            "console" if CONSOLE_METHODS.contains(&prop_text) => format!("console.{prop_text}"),
            "logger" => "logger.".to_string(),
            _ => return,
        };

        let (line, col) =
            byte_offset_to_line_col(semantic.source_text(), call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "`{pattern}` in business logic — use a `withLogging()` wrapper or domain events instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::AstCheck;
    use std::path::Path;

}
