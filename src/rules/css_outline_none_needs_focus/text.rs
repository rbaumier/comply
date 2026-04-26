use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if prop_name != "outline" && prop_name != "outline-width" && prop_name != "outline-style" {
        return;
    }
    if !is_outline_removal(&kids, source) { return; }

    // Walk up to the enclosing rule_set and inspect its selectors.
    let mut p = node.parent();
    while let Some(parent) = p {
        if parent.kind() == "rule_set" {
            let mut pc = parent.walk();
            let Some(selectors) = parent.children(&mut pc).find(|n| n.kind() == "selectors") else {
                return;
            };
            let sel_text = selectors.utf8_text(source).unwrap_or_default();
            if sel_text.contains(":focus") {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "`outline` removed outside `:focus` — keyboard users lose the focus indicator.".into(),
                Severity::Warning,
            ));
            return;
        }
        p = parent.parent();
    }
}

fn is_outline_removal(kids: &[tree_sitter::Node<'_>], source: &[u8]) -> bool {
    // Look at value siblings of `property_name`. Treat `none`, `0`, or a value
    // starting with `0` (shorthand like `0 none`) as removal.
    for n in kids
        .iter()
        .filter(|n| matches!(n.kind(), "plain_value" | "integer_value" | "float_value"))
    {
        let v = n
            .utf8_text(source)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if v == "none" || v == "0" {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_css(s, &Check)
    }

    #[test]
    fn flags_outline_none_without_focus() {
        assert_eq!(run(".btn { outline: none; }").len(), 1);
    }

    #[test]
    fn flags_outline_zero_without_focus() {
        assert_eq!(run(".btn { outline: 0; }").len(), 1);
    }

    #[test]
    fn allows_outline_none_in_focus_rule() {
        assert!(run(".btn:focus { outline: none; }").is_empty());
    }

    #[test]
    fn allows_outline_none_in_focus_visible_rule() {
        assert!(run(".btn:focus-visible { outline: none; }").is_empty());
    }

    #[test]
    fn allows_visible_outline() {
        assert!(run(".btn { outline: 2px solid blue; }").is_empty());
    }
}
