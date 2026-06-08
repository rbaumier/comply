//! no-large-snapshots AST backend — flag inline snapshots that span
//! more than `max_lines` rows.

use crate::diagnostic::{Diagnostic, Severity};

const SNAPSHOT_MATCHERS: &[&str] = &[
    "toMatchInlineSnapshot",
    "toThrowErrorMatchingInlineSnapshot",
];

crate::ast_check! { on ["call_expression"] prefilter = ["toMatchInlineSnapshot", "toThrowErrorMatchingInlineSnapshot"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let Ok(name) = property.utf8_text(source) else { return };
    if !SNAPSHOT_MATCHERS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    // Only inline snapshots (string/template literals) have a body we can measure.
    if first.kind() != "template_string" && first.kind() != "string" {
        return;
    }

    let max = ctx.config.threshold("no-large-snapshots", "max_lines", ctx.lang);
    let start = first.start_position().row;
    let end = first.end_position().row;
    let line_count = end.saturating_sub(start) + 1;

    if line_count > max {
        let pos = first.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-large-snapshots".into(),
            message: format!(
                "Inline snapshot spans {line_count} lines (max: {max}) — narrow the assertion."
            ),
            severity: Severity::Warning,
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_large_inline_snapshot() {
        let body = "\n".repeat(60);
        let src = format!("expect(x).toMatchInlineSnapshot(`{body}`)");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn allows_small_inline_snapshot() {
        let src = "expect(x).toMatchInlineSnapshot(`hello\nworld`)";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_snapshot_matcher() {
        let body = "\n".repeat(60);
        let src = format!("expect(x).toEqual(`{body}`)");
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn ignores_empty_args() {
        assert!(run_on("expect(x).toMatchInlineSnapshot()").is_empty());
    }
}
