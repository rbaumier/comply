//! Flags `style={{ ... }}` on JSX elements (object literal value).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("react-native") { return; }
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if name != "style" { return; }
    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value.kind() != "jsx_expression" { return; }
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            "object" => {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    "Inline style object allocates on every render — use `StyleSheet.create` or `useMemo`.".into(),
                    Severity::Warning,
                ));
                return;
            }
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_framework(s, &Check, "react-native")
    }

    #[test]
    fn flags_inline_style() {
        let src = "const x = <View style={{ padding: 8 }} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_stylesheet_reference() {
        let src = "const x = <View style={styles.container} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_style_with_refs() {
        let src = "const x = <View style={[styles.a, styles.b]} />;";
        assert!(run(src).is_empty());
    }
}
