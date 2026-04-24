//! Flags `renderItem={(...) => ...}` / `renderItem={function(...) {...}}` on JSX elements.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" { return; }
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if name != "renderItem" { return; }
    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value.kind() != "jsx_expression" { return; }
    // Find the inner expression.
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            "arrow_function" | "function_expression" | "function" => {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    "Inline function in `renderItem` creates a new reference every render — extract to a stable component or `useCallback`.".into(),
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
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_inline_arrow() {
        let src = "const x = <FlatList renderItem={({ item }) => <Row item={item} />} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_function_expression() {
        let src = "const x = <FlatList renderItem={function ({ item }) { return null; }} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_extracted_handler() {
        let src = "const x = <FlatList renderItem={renderRow} />;";
        assert!(run(src).is_empty());
    }
}
