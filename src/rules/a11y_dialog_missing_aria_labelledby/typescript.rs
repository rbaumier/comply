//! Walk JSX opening tags and self-closing elements. Flag those that:
//!   - have tag `dialog`, OR
//!   - have an attribute `role="dialog"` / `role="alertdialog"`, OR
//!   - have a tag named `Dialog` / ending in `Dialog` (component
//!     convention),
//! AND do NOT have any of `aria-label` / `aria-labelledby`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

fn tag_is_dialog(tag: &str) -> bool {
    tag == "dialog" || tag == "Dialog" || tag.ends_with("Dialog") || tag == "AlertDialog"
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] =>
    |node, source, ctx, diagnostics|
    let Some(tag) = jsx_element_tag_name(node, source) else { return; };

    let mut role_dialog = false;
    let mut has_label = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue; };
        match name {
            "role" => {
                if matches!(jsx_attribute_string_value(child, source), Some("dialog") | Some("alertdialog")) {
                    role_dialog = true;
                }
            }
            "aria-label" | "aria-labelledby" => {
                has_label = true;
            }
            _ => {}
        }
    }

    let is_dialog = tag_is_dialog(tag) || role_dialog;
    if !is_dialog { return; }
    if has_label { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`<{tag}>` is a dialog but has no `aria-label` or `aria-labelledby` — screen readers cannot name it."
        ),
        Severity::Error,
    ));
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

    fn run(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_native_dialog_without_label() {
        assert_eq!(run(r#"const x = <dialog open></dialog>;"#).len(), 1);
    }

    #[test]
    fn flags_role_dialog_without_label() {
        assert_eq!(run(r#"const x = <div role="dialog">x</div>;"#).len(), 1);
    }

    #[test]
    fn flags_dialog_component_without_label() {
        assert_eq!(run(r#"const x = <Dialog open>x</Dialog>;"#).len(), 1);
    }

    #[test]
    fn allows_dialog_with_aria_label() {
        assert!(run(r#"const x = <dialog aria-label="Confirm"></dialog>;"#).is_empty());
    }

    #[test]
    fn allows_dialog_with_aria_labelledby() {
        assert!(run(r#"const x = <Dialog aria-labelledby="title">x</Dialog>;"#).is_empty());
    }
}
