//! Flag `transition: all` / `transition-property: all` — the `all` keyword
//! animates every changed property, causing jank and unintended motion.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return };
    let Ok(prop_name) = prop.utf8_text(source) else { return };
    let prop_lower = prop_name.to_ascii_lowercase();

    let is_transition_all = match prop_lower.as_str() {
        "transition-property" => first_plain_value(&kids, source).is_some_and(|v| v.eq_ignore_ascii_case("all")),
        "transition" => kids.iter().any(|n| {
            n.kind() == "plain_value" && n.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("all"))
        }),
        _ => false,
    };
    if !is_transition_all { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Avoid `transition: all` — list the properties you actually animate (e.g. `transform`, `opacity`).".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

/// Return the first `plain_value` child's text, trimmed. Used to inspect the
/// first token of a declaration's value (e.g. `all` in `transition-property: all`).
fn first_plain_value<'a>(kids: &[tree_sitter::Node<'_>], source: &'a [u8]) -> Option<&'a str> {
    kids.iter()
        .find(|n| n.kind() == "plain_value")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_transition_all() {
        assert_eq!(run(".btn { transition: all 0.3s ease; }").len(), 1);
    }

    #[test]
    fn flags_transition_property_all() {
        assert_eq!(run(".btn { transition-property: all; }").len(), 1);
    }

    #[test]
    fn allows_explicit_properties() {
        assert!(run(".btn { transition: transform 0.3s, opacity 0.3s; }").is_empty());
    }

    #[test]
    fn allows_no_transition() {
        assert!(run(".btn { color: red; }").is_empty());
    }
}
