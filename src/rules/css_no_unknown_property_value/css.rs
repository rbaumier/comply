use crate::diagnostic::{Diagnostic, Severity};

const DISPLAY: &[&str] = &[
    "block",
    "inline",
    "inline-block",
    "flex",
    "inline-flex",
    "grid",
    "inline-grid",
    "none",
    "table",
    "inline-table",
    "table-row",
    "table-cell",
    "table-column",
    "table-row-group",
    "table-column-group",
    "table-header-group",
    "table-footer-group",
    "table-caption",
    "list-item",
    "contents",
    "flow",
    "flow-root",
    "ruby",
    "ruby-base",
    "ruby-text",
    "ruby-base-container",
    "ruby-text-container",
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
];

const POSITION: &[&str] = &[
    "static",
    "relative",
    "absolute",
    "fixed",
    "sticky",
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "declaration" { return; }
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    let allowed: &[&str] = match prop_name.as_str() {
        "display" => DISPLAY,
        "position" => POSITION,
        _ => return,
    };
    for value in kids.iter().filter(|n| n.kind() == "plain_value") {
        let val = value.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
        if val.starts_with("-webkit-") || val.starts_with("-moz-") || val.starts_with("-ms-") {
            continue;
        }
        if allowed.iter().any(|a| *a == val) { continue; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            value,
            super::META.id,
            format!("Unknown value `{val}` for `{prop_name}`."),
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
    fn flags_unknown_display() {
        assert_eq!(run(".a { display: fex; }").len(), 1);
    }

    #[test]
    fn allows_known_display() {
        assert!(run(".a { display: flex; }").is_empty());
    }

    #[test]
    fn flags_unknown_position() {
        assert_eq!(run(".a { position: floaty; }").len(), 1);
    }

    #[test]
    fn allows_known_position() {
        assert!(run(".a { position: relative; }").is_empty());
    }
}
