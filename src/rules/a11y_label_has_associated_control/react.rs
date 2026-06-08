//! a11y-label-has-associated-control AST backend.
//!
//! Flags `<label>` elements that lack an `htmlFor` (or `for`) attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };
    if tag != "label" {
        return;
    }

    let mut cursor = node.walk();
    let has_for = node.children(&mut cursor).any(|child| {
        matches!(jsx_attribute_name(child, source), Some("htmlFor" | "for"))
    });

    if !has_for {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-label-has-associated-control".into(),
            message: "`<label>` is missing `htmlFor` — associate it with a form control.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_label_without_htmlfor() {
        assert_eq!(run(r#"const x = <label>Name</label>;"#).len(), 1);
    }

    #[test]
    fn allows_label_with_htmlfor() {
        assert!(run(r#"const x = <label htmlFor="name-input">Name</label>;"#).is_empty());
    }

    #[test]
    fn allows_label_with_for() {
        assert!(run(r#"const x = <label for="name-input">Name</label>;"#).is_empty());
    }
}
