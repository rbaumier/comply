//! prefer-keyboard-event-key oxc backend — flag deprecated KeyboardEvent properties.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const DEPRECATED_PROPS: &[&str] = &["keyCode", "charCode", "which"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["keyCode", "charCode"])
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
        let prop_text = member.property.name.as_str();
        if !DEPRECATED_PROPS.contains(&prop_text) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Use `.key` instead of `.{prop_text}`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_event_keycode() {
        assert_eq!(run_oxc_ts("if (event.keyCode === 13) {}").len(), 1);
    }


    #[test]
    fn flags_event_which() {
        assert_eq!(run_oxc_ts("if (e.which === 27) {}").len(), 1);
    }


    #[test]
    fn flags_event_charcode() {
        assert_eq!(run_oxc_ts("const code = event.charCode;").len(), 1);
    }


    #[test]
    fn allows_event_key() {
        assert!(run_oxc_ts("if (event.key === 'Enter') {}").is_empty());
    }


    #[test]
    fn allows_comment() {
        assert!(run_oxc_ts("// event.keyCode is deprecated").is_empty());
    }
}
