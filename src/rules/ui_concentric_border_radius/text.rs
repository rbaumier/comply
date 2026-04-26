//! Walk `rule_set` blocks and flag child `border-radius` declarations that
//! omit `calc(...)` when an ancestor rule_set declares both
//! `border-radius` and `padding`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    // Does this rule_set have a non-calc `border-radius` declaration?
    let Some((radius_node, radius_is_calc)) = find_declaration(node, source, "border-radius") else { return };
    if radius_is_calc { return; }

    // Walk ancestors looking for a rule_set with BOTH `border-radius` and `padding*`.
    let mut p = node.parent();
    while let Some(parent) = p {
        if parent.kind() == "rule_set" {
            let has_parent_radius = find_declaration(parent, source, "border-radius").is_some();
            let has_parent_padding = find_declaration(parent, source, "padding").is_some()
                || find_declaration(parent, source, "padding-top").is_some()
                || find_declaration(parent, source, "padding-left").is_some()
                || find_declaration(parent, source, "padding-right").is_some()
                || find_declaration(parent, source, "padding-bottom").is_some();
            if has_parent_radius && has_parent_padding {
                let pos = radius_node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "Nested `border-radius` should use `calc(parent-radius - parent-padding)` to stay concentric.".into(),
                    severity: Severity::Warning,
                    span: Some((radius_node.byte_range().start, radius_node.byte_range().len())),
                });
                return;
            }
        }
        p = parent.parent();
    }
}

/// Return the first top-level declaration in `rule_set` whose property matches
/// `needle`, along with a flag indicating whether its value contains `calc(`.
fn find_declaration<'t>(
    rule_set: tree_sitter::Node<'t>,
    source: &[u8],
    needle: &str,
) -> Option<(tree_sitter::Node<'t>, bool)> {
    let mut c = rule_set.walk();
    let block = rule_set.children(&mut c).find(|n| n.kind() == "block")?;
    let mut bc = block.walk();
    for decl in block.children(&mut bc) {
        if decl.kind() != "declaration" { continue; }
        let mut dc = decl.walk();
        let Some(prop) = decl.children(&mut dc).find(|n| n.kind() == "property_name") else { continue };
        if !prop.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case(needle)) { continue; }
        let value_has_calc = decl
            .utf8_text(source)
            .is_ok_and(|t| t.to_ascii_lowercase().contains("calc("));
        return Some((decl, value_has_calc));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_nested_radius_without_calc() {
        let css = ".card { border-radius: 16px; padding: 8px; .inner { border-radius: 8px; } }";
        assert!(!run(css).is_empty());
    }

    #[test]
    fn allows_calc_in_nested_radius() {
        let css = ".card { border-radius: 16px; padding: 8px; .inner { border-radius: calc(16px - 8px); } }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ignores_top_level_radius() {
        assert!(run(".card { border-radius: 16px; }").is_empty());
    }
}
