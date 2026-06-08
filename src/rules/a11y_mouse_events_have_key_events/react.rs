//! a11y-mouse-events-have-key-events AST backend.
//!
//! Flags `onMouseOver` without `onFocus` and `onMouseOut` without `onBlur`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let mut has_mouse_over = false;
    let mut has_mouse_out = false;
    let mut has_focus = false;
    let mut has_blur = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match jsx_attribute_name(child, source) {
            Some("onMouseOver") => has_mouse_over = true,
            Some("onMouseOut") => has_mouse_out = true,
            Some("onFocus") => has_focus = true,
            Some("onBlur") => has_blur = true,
            _ => {}
        }
    }

    let pos = node.start_position();

    if has_mouse_over && !has_focus {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-mouse-events-have-key-events".into(),
            message: "`onMouseOver` must be accompanied by `onFocus` for keyboard accessibility.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }

    if has_mouse_out && !has_blur {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-mouse-events-have-key-events".into(),
            message: "`onMouseOut` must be accompanied by `onBlur` for keyboard accessibility.".into(),
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
    fn flags_mouse_over_without_focus() {
        let d = run(r#"const x = <div onMouseOver={handler} />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onMouseOver"));
    }

    #[test]
    fn allows_mouse_over_with_focus() {
        assert!(run(r#"const x = <div onMouseOver={handler} onFocus={handler} />;"#).is_empty());
    }

    #[test]
    fn flags_mouse_out_without_blur() {
        let d = run(r#"const x = <div onMouseOut={handler} />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onMouseOut"));
    }

    #[test]
    fn allows_mouse_out_with_blur() {
        assert!(run(r#"const x = <div onMouseOut={handler} onBlur={handler} />;"#).is_empty());
    }
}
