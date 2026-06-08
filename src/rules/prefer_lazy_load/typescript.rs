//! prefer-lazy-load AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["<img", "<iframe"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "img" && tag != "iframe" {
        return;
    }

    let mut cursor = node.walk();
    let has_loading = node.children(&mut cursor).any(|child| {
        crate::rules::jsx::jsx_attribute_name(child, source) == Some("loading")
    });
    if has_loading {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-lazy-load".into(),
        message: format!("`<{tag}>` should set `loading=\"lazy\"` to defer off-screen loads."),
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
    fn flags_img_without_loading() {
        assert_eq!(run(r#"const x = <img src="x.png" />;"#).len(), 1);
    }

    #[test]
    fn flags_iframe_without_loading() {
        assert_eq!(run(r#"const x = <iframe src="x.html" />;"#).len(), 1);
    }

    #[test]
    fn allows_img_with_lazy() {
        assert!(run(r#"const x = <img src="x.png" loading="lazy" />;"#).is_empty());
    }

    #[test]
    fn allows_img_with_eager() {
        assert!(run(r#"const x = <img src="x.png" loading="eager" />;"#).is_empty());
    }

    #[test]
    fn ignores_non_media() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
