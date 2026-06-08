//! no-sync-scripts AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["script"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "script" {
        return;
    }

    let mut cursor = node.walk();
    let mut has_src = false;
    let mut has_async = false;
    let mut has_defer = false;
    for child in node.children(&mut cursor) {
        match crate::rules::jsx::jsx_attribute_name(child, source) {
            Some("src") => has_src = true,
            Some("async") => has_async = true,
            Some("defer") => has_defer = true,
            _ => {}
        }
    }
    // Inline scripts (no src) are out of scope — different perf profile.
    if !has_src || has_async || has_defer {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-sync-scripts".into(),
        message: "`<script src>` blocks parsing — add `async` or `defer`.".into(),
        severity: Severity::Warning,
        span: None,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_sync_external_script() {
        assert_eq!(run(r#"const x = <script src="a.js" />;"#).len(), 1);
    }

    #[test]
    fn allows_async_script() {
        assert!(run(r#"const x = <script src="a.js" async />;"#).is_empty());
    }

    #[test]
    fn allows_defer_script() {
        assert!(run(r#"const x = <script src="a.js" defer />;"#).is_empty());
    }

    #[test]
    fn allows_inline_script() {
        assert!(run(r#"const x = <script>{code}</script>;"#).is_empty());
    }

    #[test]
    fn ignores_non_script() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
