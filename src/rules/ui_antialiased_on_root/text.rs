//! Walk `rule_set` nodes with root selectors (`html`, `body`, `:root`) and
//! flag the first such block that lacks `-webkit-font-smoothing`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    // Only fire once per file; skip after the first root block is classified.
    // We can't carry state across nodes, so emit deterministically on every
    // matching root rule that lacks the declaration — then dedupe to one.
    if !diagnostics.is_empty() { return; }
    // Skip nested rule_sets (those inside @media) — we only care about the
    // top-level root-element selectors so the diagnostic matches the original
    // "first offender wins" behaviour.
    if let Some(parent) = node.parent()
        && parent.kind() != "stylesheet" { return }

    let mut cursor = node.walk();
    let Some(selectors) = node.children(&mut cursor).find(|c| c.kind() == "selectors") else { return };
    let Ok(sel_text) = selectors.utf8_text(source) else { return };
    let sel = sel_text.trim();
    if !selector_is_root(sel) { return; }

    // Inspect the block for `-webkit-font-smoothing` declaration.
    let mut body_cursor = node.walk();
    let Some(block) = node.children(&mut body_cursor).find(|c| c.kind() == "block") else { return };
    let mut decl_cursor = block.walk();
    let has_smoothing = block.children(&mut decl_cursor).any(|child| {
        if child.kind() != "declaration" { return false; }
        let mut c = child.walk();
        child.children(&mut c).any(|n| {
            n.kind() == "property_name"
                && n.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("-webkit-font-smoothing"))
        })
    });
    if has_smoothing { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Root selector `{sel}` is missing `-webkit-font-smoothing: antialiased` — text will render heavier on macOS/iOS."
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn selector_is_root(selector: &str) -> bool {
    selector.split(',').any(|part| {
        let t = part.trim();
        t == "html" || t == "body" || t == ":root"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_body_without_smoothing() {
        assert_eq!(run("body { font-family: sans-serif; }").len(), 1);
    }

    #[test]
    fn flags_root_without_smoothing() {
        assert_eq!(run(":root { --gap: 8px; }").len(), 1);
    }

    #[test]
    fn allows_body_with_smoothing() {
        assert!(run("body { -webkit-font-smoothing: antialiased; }").is_empty());
    }

    #[test]
    fn ignores_non_root_rules() {
        assert!(run(".card { color: red; }").is_empty());
    }
}
