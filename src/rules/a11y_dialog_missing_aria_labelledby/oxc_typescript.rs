use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

pub struct Check;

/// True for the *native* `<dialog>` HTML element only.
///
/// Library / user-defined components named `Dialog` / `*Dialog`
/// (Base UI, Radix, coss, shadcn, app wrappers like `<CreateUserDialog>`)
/// own their own a11y wiring — they read `<DialogTitle>` from their
/// children and assign `aria-labelledby` themselves. This rule cannot
/// see across the component boundary, so flagging those components
/// produces a flood of false positives.
///
/// For unambiguous dialog intent on a non-native tag, authors can still
/// set `role="dialog"` / `role="alertdialog"` and the rule will catch
/// missing labels there.
fn tag_is_dialog(tag: &str) -> bool {
    tag == "dialog"
}

fn jsx_tag_name<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<&'a str> {
    match &opening.name {
        oxc_ast::ast::JSXElementName::Identifier(id) => Some(id.name.as_str()),
        oxc_ast::ast::JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        oxc_ast::ast::JSXElementName::NamespacedName(ns) => Some(ns.name.name.as_str()),
        oxc_ast::ast::JSXElementName::MemberExpression(member) => {
            Some(member.property.name.as_str())
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let Some(tag) = jsx_tag_name(opening) else {
            return;
        };

        let mut role_dialog = false;
        let mut has_label = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "role" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value
                        && (s.value.as_str() == "dialog" || s.value.as_str() == "alertdialog") {
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
        if !is_dialog {
            return;
        }
        if has_label {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`<{tag}>` is a dialog but has no `aria-label` or `aria-labelledby` — screen readers cannot name it."
            ),
            severity: Severity::Error,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_native_dialog_without_label() {
        let src = r#"const x = <dialog open>Hi</dialog>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_native_dialog_with_aria_labelledby() {
        let src = r#"const x = <dialog aria-labelledby="t">Hi</dialog>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_role_dialog_without_label() {
        let src = r#"const x = <div role="dialog">Hi</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_base_ui_dialog_component_with_title_child() {
        // Regression for rbaumier/comply#14 — Base UI / coss Dialog wires
        // aria-labelledby from <DialogTitle> automatically; the rule cannot
        // see across the component boundary, so it must stay silent on
        // capitalised component tags.
        let src = r#"
            const x = <Dialog open={open} onOpenChange={setOpen}>
              <DialogPopup>
                <DialogTitle>Nouvel utilisateur</DialogTitle>
              </DialogPopup>
            </Dialog>;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_app_level_dialog_wrapper() {
        // Regression for rbaumier/comply#14 — application-level wrappers
        // around the primitive (e.g. <CreateUserDialog>) must not fire.
        let src = r#"const x = <CreateUserDialog />;"#;
        assert!(run(src).is_empty());
    }
}
