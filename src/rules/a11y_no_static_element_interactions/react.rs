//! a11y-no-static-element-interactions AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const STATIC_ELEMENTS: &[&str] = &["div", "span"];

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if !STATIC_ELEMENTS.contains(&tag) {
        return;
    }

    let mut has_onclick = false;
    let mut has_role = false;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name == "onClick" {
            has_onclick = true;
        }
        if name == "role" {
            has_role = true;
        }
    }

    if has_onclick && !has_role {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-no-static-element-interactions".into(),
            message: format!(
                "Static element `<{tag}>` has `onClick` without a `role` attribute."
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_div_onclick_without_role() {
        let d = run(r#"const x = <div onClick={handler}>Click</div>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("div"));
    }

    #[test]
    fn flags_span_onclick_without_role() {
        let d = run(r#"const x = <span onClick={handler}>Click</span>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("span"));
    }

    #[test]
    fn allows_div_with_role() {
        assert!(run(r#"const x = <div role="button" onClick={handler}>Click</div>;"#).is_empty());
    }

    #[test]
    fn allows_button() {
        assert!(run(r#"const x = <button onClick={handler}>Click</button>;"#).is_empty());
    }
}
