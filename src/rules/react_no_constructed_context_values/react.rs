//! react-no-constructed-context-values AST backend.
//!
//! Flags `<Provider value={{ ... }}>` or `<Provider value={[ ... ]}>` —
//! inline object/array literals passed to a context Provider's `value`
//! prop create a new reference every render, forcing all consumers to
//! re-render.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    // Match jsx_attribute nodes named "value"
    let Some(name_node) = node.child(0) else { return };
    let Ok(name_text) = name_node.utf8_text(source) else { return };
    if name_text != "value" {
        return;
    }

    // The attribute must be on a Provider element.
    let Some(parent) = node.parent() else { return };
    let tag_kind = parent.kind();
    if tag_kind != "jsx_opening_element" && tag_kind != "jsx_self_closing_element" {
        return;
    }
    let Some(tag_name) = parent.child_by_field_name("name") else { return };
    let Ok(tag_text) = tag_name.utf8_text(source) else { return };
    if !tag_text.contains("Provider") {
        return;
    }

    // Check if value is a jsx_expression containing an object or array literal.
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value_node.kind() != "jsx_expression" {
        return;
    }

    // The jsx_expression wraps `{ expr }` — look at the first named child.
    let mut cursor = value_node.walk();
    let is_inline = value_node.named_children(&mut cursor).any(|child| {
        child.kind() == "object" || child.kind() == "array"
    });

    if is_inline {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-constructed-context-values".into(),
            message: "Context Provider `value` is an inline object/array — \
                      a new reference is created every render, causing all \
                      consumers to re-render. Memoize with `useMemo`."
                .into(),
            severity: Severity::Warning,
            span: None,
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
    fn flags_inline_object() {
        let src = r#"const x = <MyContext.Provider value={{ foo: 1 }}>child</MyContext.Provider>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_array() {
        let src = r#"const x = <ThemeProvider value={[theme, setTheme]}>child</ThemeProvider>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_memoized_value() {
        let src = r#"const x = <MyContext.Provider value={memoizedValue}>child</MyContext.Provider>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_provider() {
        let src = r#"const x = <Foo value={{ bar: 1 }} />;"#;
        assert!(run(src).is_empty());
    }
}
