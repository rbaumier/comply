//! CSS backend — flag class selectors whose name suggests numeric data
//! but whose declaration block lacks `font-variant-numeric: tabular-nums`.
//!
//! tree-sitter-css gives us a structured `rule_set { selectors block }`,
//! but the `font-variant-numeric: tabular-nums` declaration is recognised
//! as plain `value` text — so we keep the check at the rule_set level and
//! search the block text for the directive.

use crate::diagnostic::{Diagnostic, Severity};

const DATA_HINTS: &[&str] = &[
    "counter", "count", "price", "amount", "metric", "stat", "number", "value-display",
];

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    let Some(selectors) = node.child_by_field_name("selectors")
        .or_else(|| node.named_child(0))
    else { return };
    let Ok(selector_text) = selectors.utf8_text(source) else { return };
    let lower_sel = selector_text.to_ascii_lowercase();
    if !DATA_HINTS.iter().any(|h| lower_sel.contains(h)) { return; }

    // Block text — covers `font-variant-numeric: tabular-nums` and
    // shorthand `tabular-nums` from postcss-tailwind-style transforms.
    let Ok(full) = node.utf8_text(source) else { return };
    let lower_full = full.to_ascii_lowercase();
    if lower_full.contains("tabular-nums") || lower_full.contains("tabular-numbers") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Selector `{selector_text}` suggests numeric data but the rule lacks `font-variant-numeric: tabular-nums` — digits will jitter between updates."
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    #[test]
    fn flags_counter_selector_without_directive() {
        let src = ".counter { color: red; }";
        assert_eq!(run_css(src, &Check).len(), 1);
    }

    #[test]
    fn allows_counter_with_tabular_nums() {
        let src = ".counter { font-variant-numeric: tabular-nums; }";
        assert!(run_css(src, &Check).is_empty());
    }

    #[test]
    fn ignores_unrelated_selector() {
        let src = ".card { color: red; }";
        assert!(run_css(src, &Check).is_empty());
    }
}
