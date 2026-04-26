use crate::diagnostic::{Diagnostic, Severity};

fn inside_media(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "media_statement" {
            return true;
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["feature_query", "parenthesized_value"] => |node, source, ctx, diagnostics|
    if !inside_media(node) { return; }
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    // Find feature name and value pair.
    let name_node = kids.iter().find(|n| matches!(n.kind(), "feature_name" | "plain_value"));
    let Some(name_node) = name_node else { return; };
    let name = name_node.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    let allowed: &[&str] = match name.as_str() {
        "prefers-color-scheme" => &["light", "dark", "no-preference"],
        "orientation" => &["portrait", "landscape"],
        _ => return,
    };
    // Find the value node: any plain_value other than the name node.
    for v in &kids {
        if v.kind() != "plain_value" { continue; }
        if v.start_byte() == name_node.start_byte() { continue; }
        let val = v.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
        if val.is_empty() { continue; }
        if allowed.iter().any(|a| *a == val) { continue; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            v,
            super::META.id,
            format!("Invalid value `{val}` for `{name}`."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_invalid_orientation() {
        let css = "@media (orientation: diagonal) { .a { color: red; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_valid_orientation() {
        let css = "@media (orientation: portrait) { .a { color: red; } }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_valid_prefers_color_scheme() {
        let css = "@media (prefers-color-scheme: dark) { .a { color: red; } }";
        assert!(run(css).is_empty());
    }
}
