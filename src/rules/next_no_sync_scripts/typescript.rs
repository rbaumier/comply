//! next-no-sync-scripts backend.
//!
//! Flags `<script src="…">` tags that lack `async`/`defer`. In a Next.js
//! app this should be replaced by `<Script>` from `next/script` so the
//! framework can choose a strategy (`afterInteractive`, `lazyOnload`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn tag_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("name")?.utf8_text(source).ok()
}

fn has_attr(node: tree_sitter::Node, source: &[u8], wanted: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(name_node) = child.child(0) else { continue };
        let Ok(name) = name_node.utf8_text(source) else { continue };
        if name == wanted {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    let Some(tag) = tag_name(node, source) else { return };
    if tag != "script" {
        return;
    }
    if !has_attr(node, source, "src") {
        return;
    }
    if has_attr(node, source, "async") || has_attr(node, source, "defer") {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-sync-scripts".into(),
        message: "Use `<Script>` from `next/script` instead of a synchronous `<script src>` tag.".into(),
        severity: Severity::Warning,
        span: Some((range.start, range.len())),
    });
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
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", project, &FileCtx::default())
    }

    #[test]
    fn flags_sync_script_with_src() {
        let src = "export default function Page() { return <script src='/a.js' />; }";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_async_script() {
        let src = "export default function Page() { return <script src='/a.js' async />; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_defer_script() {
        let src = "export default function Page() { return <script src='/a.js' defer />; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_inline_script_without_src() {
        let src = "export default function Page() { return <script>{`alert(1)`}</script>; }";
        assert!(run(src, &next_project()).is_empty());
    }
}
