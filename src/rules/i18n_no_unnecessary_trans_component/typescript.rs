//! i18n-no-unnecessary-trans-component AST backend.
//!
//! Flags `<Trans ...>Plain text</Trans>` when every named child is a
//! `jsx_text` node (i.e. literal text with no JSX elements or
//! interpolations). In that case `<Trans>` adds no value over the
//! plain `t('key')` call and misleads readers into thinking the
//! children contain inline markup. Self-closing `<Trans />` and
//! `<Trans>{...}</Trans>` interpolations are left alone because they
//! rely on the component's runtime behaviour.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_element"] prefilter = ["Trans"] => |node, source, ctx, diagnostics|
    let Some(opening) = node.child(0) else { return };
    if opening.kind() != "jsx_opening_element" {
        return;
    }
    let Some(name_node) = opening.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };
    if tag != "Trans" {
        return;
    }

    // Iterate over the element's children (skipping the opening and
    // closing tags). The element is unnecessary iff there is at least
    // one text child and every non-tag child is a `jsx_text` node with
    // non-whitespace content.
    let mut cursor = node.walk();
    let mut has_text = false;
    let mut all_plain = true;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "jsx_opening_element" | "jsx_closing_element" => {}
            "jsx_text" => {
                let Ok(text) = child.utf8_text(source) else {
                    all_plain = false;
                    break;
                };
                if !text.trim().is_empty() {
                    has_text = true;
                }
            }
            _ => {
                all_plain = false;
                break;
            }
        }
    }

    if !has_text || !all_plain {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`<Trans>` with only plain-text children is unnecessary. \
                  Use `t('key')` instead — reserve `<Trans>` for JSX interpolation."
            .into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_trans_with_key_and_text() {
        let src = r#"const x = <Trans i18nKey="greeting">Hello</Trans>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_trans_with_only_text() {
        let src = r#"const x = <Trans>Just text</Trans>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_trans_with_jsx_child() {
        let src = r#"const x = <Trans><b>bold</b> text</Trans>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_trans_with_expression_child() {
        let src = r#"const x = <Trans>{userName}</Trans>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_self_closing_trans() {
        let src = r#"const x = <Trans i18nKey="x" />;"#;
        assert!(run(src).is_empty());
    }
}
