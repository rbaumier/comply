//! next-no-hardcoded-revalidate-zero backend.
//!
//! Flags top-level `export const revalidate = 0;` in app router segment
//! config. The `dynamic = 'force-dynamic'` form is more explicit and
//! survives type-narrowing in editor tooling.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn declarator_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("name")?.utf8_text(source).ok()
}

fn declarator_value<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let value = node.child_by_field_name("value")?;
    value.utf8_text(source).ok().map(str::trim)
}

fn is_top_level_export(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    let mut hit_export = false;
    while let Some(parent) = current {
        match parent.kind() {
            "export_statement" => hit_export = true,
            "program" => return hit_export,
            _ => {}
        }
        current = parent.parent();
    }
    false
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if !is_top_level_export(node) {
        return;
    }
    let Some(name) = declarator_name(node, source) else { return };
    if name != "revalidate" {
        return;
    }
    let Some(value) = declarator_value(node, source) else { return };
    if value != "0" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-hardcoded-revalidate-zero".into(),
        message: "Replace `export const revalidate = 0` with `export const dynamic = 'force-dynamic'`.".into(),
        severity: Severity::Warning,
        span: Some((range.start, range.len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_and_file(
            source,
            &Check,
            project,
            &FileCtx::default(),
        )
    }

    #[test]
    fn flags_revalidate_zero() {
        let src = "export const revalidate = 0;";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_revalidate_60() {
        let src = "export const revalidate = 60;";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_dynamic_force_dynamic() {
        let src = "export const dynamic = 'force-dynamic';";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_local_revalidate_zero() {
        let src = "function f() { const revalidate = 0; return revalidate; }";
        assert!(run(src, &next_project()).is_empty());
    }
}
