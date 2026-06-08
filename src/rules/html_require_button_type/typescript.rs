//! html-require-button-type AST backend.
//!
//! Walks JSX opening / self-closing elements; whenever the tag is
//! `button`, requires a `type` attribute to be present.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["button"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "button" {
        return;
    }

    let mut cursor = node.walk();
    let has_type = node.children(&mut cursor).any(|child| {
        crate::rules::jsx::jsx_attribute_name(child, source) == Some("type")
    });

    if has_type {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "html-require-button-type".into(),
        message: "`<button>` is missing an explicit `type` attribute (defaults to `submit` inside forms).".into(),
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
    fn flags_button_without_type() {
        assert_eq!(run(r#"const x = <button>Save</button>;"#).len(), 1);
    }

    #[test]
    fn flags_self_closing_button_without_type() {
        assert_eq!(run(r#"const x = <button />;"#).len(), 1);
    }

    #[test]
    fn allows_button_with_type() {
        assert!(run(r#"const x = <button type="button">Save</button>;"#).is_empty());
    }

    #[test]
    fn allows_button_type_submit() {
        assert!(run(r#"const x = <button type="submit">Go</button>;"#).is_empty());
    }

    #[test]
    fn ignores_non_button() {
        assert!(run(r#"const x = <div>Save</div>;"#).is_empty());
    }
}
