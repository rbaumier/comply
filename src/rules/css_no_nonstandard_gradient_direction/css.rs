use crate::diagnostic::{Diagnostic, Severity};

const SIDES: &[&str] = &["top", "bottom", "left", "right"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(name_node) = node.children(&mut c).find(|n| n.kind() == "function_name") else { return; };
    let name = name_node.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if !matches!(name.as_str(), "linear-gradient" | "repeating-linear-gradient") { return; }
    let mut c2 = node.walk();
    let Some(args) = node.children(&mut c2).find(|n| n.kind() == "arguments") else { return; };
    // Look at the first plain_value child of arguments.
    let mut ac = args.walk();
    let Some(first_value) = args.children(&mut ac).find(|n| n.kind() == "plain_value") else { return; };
    let first = first_value.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if SIDES.iter().any(|s| *s == first) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &first_value,
            super::META.id,
            format!("Bare direction `{first}`; use `to {first}` for the standard syntax."),
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
    fn flags_bare_direction() {
        assert_eq!(
            run(".a { background: linear-gradient(top, red, blue); }").len(),
            1
        );
    }

    #[test]
    fn allows_to_direction() {
        assert!(run(".a { background: linear-gradient(to top, red, blue); }").is_empty());
    }

    #[test]
    fn allows_angle() {
        assert!(run(".a { background: linear-gradient(45deg, red, blue); }").is_empty());
    }
}
