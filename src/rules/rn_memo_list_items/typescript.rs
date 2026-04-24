//! Flags `renderItem={SomeComponent}` when `SomeComponent` is defined in the
//! same file without being wrapped in `memo(...)` / `React.memo(...)`.

use crate::diagnostic::{Diagnostic, Severity};

fn source_wraps_in_memo(source: &[u8], ident: &str) -> bool {
    let Ok(text) = std::str::from_utf8(source) else { return true };
    // Look for `memo(Ident)` or `React.memo(Ident)` or `const Ident = memo(`.
    let patterns = [
        format!("memo({ident})"),
        format!("React.memo({ident})"),
        format!("const {ident} = memo("),
        format!("const {ident} = React.memo("),
        format!("let {ident} = memo("),
        format!("var {ident} = memo("),
    ];
    patterns.iter().any(|p| text.contains(p.as_str()))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" { return; }
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if name != "renderItem" { return; }
    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value.kind() != "jsx_expression" { return; }
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            "identifier" => {
                let Ok(ident) = child.utf8_text(source) else { return };
                // Only flag PascalCase identifiers (component convention).
                if !ident.chars().next().is_some_and(|c| c.is_ascii_uppercase()) { return; }
                if source_wraps_in_memo(source, ident) { return; }
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    format!("List item component `{ident}` should be wrapped in `React.memo(...)`."),
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
    fn flags_unmemoized_component() {
        let src = "function Row() { return null; }\nconst x = <FlatList renderItem={Row} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_memo_wrapped() {
        let src = "const Row = memo(function Row() { return null; });\nconst x = <FlatList renderItem={Row} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_react_memo_wrapped() {
        let src = "const Row = React.memo(RowImpl);\nconst x = <FlatList renderItem={Row} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_pascal_reference() {
        // A camelCase identifier likely points to a stable callback, not a component.
        let src = "const x = <FlatList renderItem={renderRow} />;";
        assert!(run(src).is_empty());
    }
}
