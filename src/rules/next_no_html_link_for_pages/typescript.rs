//! next-no-html-link-for-pages backend.
//!
//! Flags `<a href="/path">` for internal-looking routes (starts with `/`,
//! not protocol-prefixed, no `target="_blank"`). External links and
//! hash/mailto/tel anchors are ignored.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn tag_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("name")?.utf8_text(source).ok()
}

fn attr_value<'a>(node: tree_sitter::Node, source: &'a [u8], wanted: &str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let name = child.child(0)?.utf8_text(source).ok()?;
        if name != wanted {
            continue;
        }
        let value_node = child.child(2)?;
        let raw = value_node.utf8_text(source).ok()?;
        return Some(raw.trim_matches(|c| c == '"' || c == '\'' || c == '{' || c == '}'));
    }
    None
}

fn has_attr(node: tree_sitter::Node, source: &[u8], wanted: &str) -> bool {
    attr_value(node, source, wanted).is_some()
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    let Some(tag) = tag_name(node, source) else { return };
    if tag != "a" {
        return;
    }
    let Some(href) = attr_value(node, source, "href") else { return };

    if !href.starts_with('/') || href.starts_with("//") {
        return;
    }
    if has_attr(node, source, "target") {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-html-link-for-pages".into(),
        message: "Use `<Link>` from `next/link` for internal routes — `<a>` triggers a full reload.".into(),
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
    fn flags_internal_anchor() {
        let src = "export default function Nav() { return <a href='/about'>About</a>; }";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_external_anchor() {
        let src = "export default function Nav() { return <a href='https://x.com'>X</a>; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_anchor_with_target() {
        let src = "export default function Nav() { return <a href='/about' target='_blank'>About</a>; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export default function Nav() { return <a href='/about'>About</a>; }";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
