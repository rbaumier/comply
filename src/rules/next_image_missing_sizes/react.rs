//! Detects `<Image fill />` (or `<Image fill={true}>`) without a `sizes`
//! prop. The `fill` mode stretches the image to its container, but without
//! `sizes` Next.js cannot pick the right `srcset` candidate.

use crate::diagnostic::{Diagnostic, Severity};

fn has_jsx_attribute(element: tree_sitter::Node, source: &[u8], attr_name: &str) -> bool {
    let mut cursor = element.walk();
    element.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" {
            return false;
        }
        crate::rules::jsx::jsx_attribute_name(child, source) == Some(attr_name)
    })
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let tag_name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if tag_name != "Image" {
        return;
    }

    if !has_jsx_attribute(node, source, "fill") {
        return;
    }
    if has_jsx_attribute(node, source, "sizes") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`<Image fill />` without `sizes` — the browser downloads the largest image. Add a `sizes` prop.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_fill_without_sizes() {
        let diags = run(r#"function App() { return <Image src="/a.png" fill alt="" />; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_fill_true_without_sizes() {
        let diags = run(r#"function App() { return <Image src="/a.png" fill={true} alt="" />; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_fill_with_opening_tag() {
        let diags = run(r#"function App() { return <Image src="/a.png" fill alt=""></Image>; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_fill_with_sizes() {
        assert!(
            run(r#"function App() { return <Image src="/a.png" fill sizes="100vw" alt="" />; }"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_image_without_fill() {
        assert!(run(r#"function App() { return <Image src="/a.png" width={100} height={100} alt="" />; }"#).is_empty());
    }

    #[test]
    fn ignores_other_components() {
        assert!(run(r#"function App() { return <img src="/a.png" />; }"#).is_empty());
    }
}
