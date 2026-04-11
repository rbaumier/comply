//! a11y-html-has-lang AST backend.
//!
//! Flags `<html>` elements that are missing a `lang` attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };
    if tag != "html" {
        return;
    }

    // Look for a `lang` attribute.
    let mut cursor = node.walk();
    let has_lang = node.children(&mut cursor).any(|child| {
        jsx_attribute_name(child, source) == Some("lang")
    });

    if !has_lang {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-html-has-lang".into(),
            message: "`<html>` is missing a `lang` attribute.".into(),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_html_without_lang() {
        assert_eq!(run("const x = <html><body /></html>;").len(), 1);
    }

    #[test]
    fn allows_html_with_lang() {
        assert!(run(r#"const x = <html lang="en"><body /></html>;"#).is_empty());
    }

    #[test]
    fn allows_html_with_lang_multiline() {
        let src = r#"const x = <html
  lang="en"
><body /></html>;"#;
        assert!(run(src).is_empty());
    }
}
