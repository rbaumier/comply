//! prefer-keyboard-event-key oxc backend — flag deprecated KeyboardEvent properties.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_plain_assignment_target};
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !DEPRECATED_PROPS.contains(&prop_text) {
            return;
        }
        // The deprecation is about reading the key off a `KeyboardEvent`. A write
        // target stores a same-named field on another object. Test helpers build
        // synthetic events this way, and `e.which = e.keyCode` normalises one
        // legacy field from another.
        if is_plain_assignment_target(node, semantic) {
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
            severity: Severity::Error,
            span: None,
        });
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_keycode_read() {
        assert_eq!(run("if (e.keyCode === 13) { fire(); }").len(), 1);
    }

    #[test]
    fn flags_charcode_and_which_reads() {
        assert_eq!(run("send(e.charCode, e.which);").len(), 2);
    }

    #[test]
    fn allows_write_target() {
        assert!(run("row.keyCode = 13;").is_empty());
        assert!(run("row.charCode = 65;").is_empty());
        assert!(run("row.which = 1;").is_empty());
    }

    /// The `e.which = e.keyCode` normalisation from Mousetrap: the store on the
    /// left is not a use of the deprecated API, the load on the right is.
    #[test]
    fn flags_only_the_read_side_of_a_write() {
        let found = run("e.which = e.keyCode;");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].column, 11);
    }

    #[test]
    fn flags_compound_assignment_because_it_reads_first() {
        assert_eq!(run("row.keyCode += 1;").len(), 1);
        assert_eq!(run("row.charCode ||= 32;").len(), 1);
    }

    #[test]
    fn flags_update_expression_because_it_reads_first() {
        assert_eq!(run("row.keyCode++;").len(), 1);
        assert_eq!(run("--row.charCode;").len(), 1);
    }
}
