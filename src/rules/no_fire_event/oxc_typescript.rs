use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "fireEvent" {
            return;
        }
        let path_str = ctx.path.to_string_lossy();
        if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `userEvent` over `fireEvent` — `fireEvent` dispatches a single synthetic event and skips intermediate browser events.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_fire_event_in_test() {
        let diags = run_on(
            "components/__tests__/button.test.tsx",
            "fireEvent.click(button)",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_user_event() {
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "userEvent.click(button)"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run_on("components/button.tsx", "fireEvent.click(button)").is_empty());
    }
}
