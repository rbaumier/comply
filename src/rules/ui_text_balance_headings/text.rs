//! Flag heading rule_sets (h1–h6) whose block omits `text-wrap: balance`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(selectors) = node.children(&mut c).find(|n| n.kind() == "selectors") else { return };
    let Ok(sel_text) = selectors.utf8_text(source) else { return };
    if !selector_targets_heading(sel_text) { return; }

    let mut bc = node.walk();
    let Some(block) = node.children(&mut bc).find(|n| n.kind() == "block") else { return };
    if block_has_text_wrap(block, source) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Heading selector `{}` is missing `text-wrap: balance` — long titles will orphan the last word.",
            sel_text.trim()
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn selector_targets_heading(selector: &str) -> bool {
    selector.split(',').any(|part| {
        let part = part.trim();
        if part.is_empty() { return false; }
        let last = part
            .rsplit(|c: char| c.is_whitespace() || c == '>' || c == '+' || c == '~')
            .next()
            .unwrap_or("");
        let base = last.split([':', '.', '[']).next().unwrap_or("");
        matches!(base, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
    })
}

fn block_has_text_wrap(block: tree_sitter::Node, source: &[u8]) -> bool {
    let mut c = block.walk();
    block.children(&mut c).any(|decl| {
        if decl.kind() != "declaration" { return false; }
        let mut dc = decl.walk();
        decl.children(&mut dc).any(|n| {
            n.kind() == "property_name"
                && n.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("text-wrap"))
        })
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
    fn flags_h1_without_balance() {
        assert_eq!(run("h1 { font-size: 3rem; }").len(), 1);
    }

    #[test]
    fn flags_h2_h3_group() {
        assert_eq!(run("h2, h3 { font-weight: 600; }").len(), 1);
    }

    #[test]
    fn allows_heading_with_balance() {
        assert!(run("h1 { font-size: 3rem; text-wrap: balance; }").is_empty());
    }

    #[test]
    fn ignores_non_heading() {
        assert!(run(".title { font-size: 3rem; }").is_empty());
    }
}
