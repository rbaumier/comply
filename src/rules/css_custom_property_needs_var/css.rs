use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    // Skip the property name (first child); only inspect value-side children.
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default();
    // The property itself starts with `--` for custom property *definitions* — that's allowed.
    if prop_name.starts_with("--") { return; }
    for value in &kids {
        if value.kind() != "plain_value" { continue; }
        let txt = value.utf8_text(source).unwrap_or_default();
        if txt.starts_with("--") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                value,
                super::META.id,
                format!("Custom property `{txt}` must be wrapped in `var()`."),
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
    fn flags_bare_custom_property() {
        assert_eq!(run(".a { color: --brand; }").len(), 1);
    }

    #[test]
    fn allows_var_wrapped() {
        assert!(run(".a { color: var(--brand); }").is_empty());
    }

    #[test]
    fn allows_custom_property_definition() {
        assert!(run(".a { --brand: red; }").is_empty());
    }
}
