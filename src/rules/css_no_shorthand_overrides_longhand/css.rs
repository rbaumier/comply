use crate::diagnostic::{Diagnostic, Severity};

fn longhand_of(prop: &str) -> Option<&'static str> {
    let p = prop;
    if p.starts_with("margin-")
        && matches!(
            p,
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left"
        )
    {
        return Some("margin");
    }
    if p.starts_with("padding-")
        && matches!(
            p,
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left"
        )
    {
        return Some("padding");
    }
    if matches!(
        p,
        "border-width"
            | "border-style"
            | "border-color"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style"
            | "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color"
    ) {
        return Some("border");
    }
    if matches!(
        p,
        "background-color"
            | "background-image"
            | "background-position"
            | "background-size"
            | "background-repeat"
            | "background-origin"
            | "background-clip"
            | "background-attachment"
    ) {
        return Some("background");
    }
    if matches!(
        p,
        "font-style"
            | "font-variant"
            | "font-weight"
            | "font-stretch"
            | "font-size"
            | "line-height"
            | "font-family"
    ) {
        return Some("font");
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "block" { return; }
    let mut c = node.walk();
    let decls: Vec<_> = node
        .children(&mut c)
        .filter(|n| n.kind() == "declaration")
        .collect();
    let mut props: Vec<(String, tree_sitter::Node)> = Vec::new();
    for decl in &decls {
        let mut dc = decl.walk();
        if let Some(prop) = decl.children(&mut dc).find(|n| n.kind() == "property_name") {
            let name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
            props.push((name, *decl));
        }
    }
    for (i, (name, decl)) in props.iter().enumerate() {
        // For each property, check if it's a shorthand and if any earlier prop is a longhand of it.
        let shorthand = name.as_str();
        let earlier_has_longhand = props.iter().take(i).any(|(earlier, _)| {
            longhand_of(earlier).is_some_and(|sh| sh == shorthand)
        });
        if earlier_has_longhand {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                decl,
                super::META.id,
                format!("Shorthand `{shorthand}` overrides a longhand declared above."),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_padding_after_padding_left() {
        assert_eq!(run(".a { padding-left: 10px; padding: 20px; }").len(), 1);
    }

    #[test]
    fn allows_padding_before_padding_left() {
        assert!(run(".a { padding: 20px; padding-left: 10px; }").is_empty());
    }
}
