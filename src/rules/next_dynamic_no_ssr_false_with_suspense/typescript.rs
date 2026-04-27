//! next-dynamic-no-ssr-false-with-suspense backend.
//!
//! Flags `dynamic(import('…'), { ssr: false })` calls. Modern app router
//! code should reach for `<Suspense>` boundaries instead of disabling SSR
//! wholesale.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn callee_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("function")?.utf8_text(source).ok()
}

fn second_argument(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut iter = args.named_children(&mut cursor);
    let _first = iter.next()?;
    iter.next()
}

fn options_object_has_ssr_false(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "object" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else { continue };
        let Ok(key_text) = key.utf8_text(source) else { continue };
        if key_text.trim_matches(|c| c == '"' || c == '\'') != "ssr" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else { continue };
        let Ok(value_text) = value.utf8_text(source) else { continue };
        if value_text.trim() == "false" {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    let Some(callee) = callee_text(node, source) else { return };
    if callee != "dynamic" {
        return;
    }
    let Some(options) = second_argument(node) else { return };
    if !options_object_has_ssr_false(options, source) {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-dynamic-no-ssr-false-with-suspense".into(),
        message: "Replace `dynamic(..., { ssr: false })` with a `<Suspense>` boundary, or move the lazy import into a client component.".into(),
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
    fn flags_dynamic_with_ssr_false() {
        let src = "const C = dynamic(() => import('./c'), { ssr: false });";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_dynamic_with_ssr_true() {
        let src = "const C = dynamic(() => import('./c'), { ssr: true });";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_dynamic_without_options() {
        let src = "const C = dynamic(() => import('./c'));";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "const C = dynamic(() => import('./c'), { ssr: false });";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
