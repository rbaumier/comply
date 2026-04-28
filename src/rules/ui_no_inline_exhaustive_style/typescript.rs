//! Flags JSX `style={{...}}` with > 8 properties.

use crate::diagnostic::{Diagnostic, Severity};

const INLINE_STYLE_PROPERTY_THRESHOLD: usize = 8;

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if attr_name != "style" {
        return;
    }

    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    // style={...} → jsx_expression wrapping the object
    let obj = if value_node.kind() == "jsx_expression" {
        match value_node.named_child(0) {
            Some(o) => o,
            None => return,
        }
    } else {
        return;
    };
    if obj.kind() != "object" {
        return;
    }

    let mut cursor = obj.walk();
    let prop_count = obj
        .children(&mut cursor)
        .filter(|c| c.kind() == "pair" || c.kind() == "shorthand_property_identifier")
        .count();

    if prop_count <= INLINE_STYLE_PROPERTY_THRESHOLD {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Inline `style` has {prop_count} properties — extract to a CSS class or styled component."
        ),
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
    fn flags_exhaustive_inline_style() {
        let src = r#"<div style={{
            color: 'red',
            fontSize: 14,
            fontWeight: 'bold',
            margin: 0,
            padding: 10,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            border: '1px solid',
        }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_few_inline_styles() {
        assert!(run(r#"<div style={{ color: 'red', fontSize: 14 }} />"#).is_empty());
    }

    #[test]
    fn allows_exactly_8() {
        let src = r#"<div style={{
            color: 'red',
            fontSize: 14,
            fontWeight: 'bold',
            margin: 0,
            padding: 10,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
        }} />"#;
        assert!(run(src).is_empty());
    }
}
