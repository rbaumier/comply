//! html-require-img-alt AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["img"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "img" {
        return;
    }

    let mut cursor = node.walk();
    let has_alt = node.children(&mut cursor).any(|child| {
        crate::rules::jsx::jsx_attribute_name(child, source) == Some("alt")
    });
    if has_alt {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "html-require-img-alt".into(),
        message: "`<img>` is missing an `alt` attribute.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_img_without_alt() {
        assert_eq!(run(r#"const x = <img src="x.png" />;"#).len(), 1);
    }

    #[test]
    fn allows_img_with_alt() {
        assert!(run(r#"const x = <img src="x.png" alt="logo" />;"#).is_empty());
    }

    #[test]
    fn allows_empty_alt_for_decorative() {
        assert!(run(r#"const x = <img src="x.png" alt="" />;"#).is_empty());
    }

    #[test]
    fn ignores_non_img() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
