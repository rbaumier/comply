//! CSS backend — flag class selectors whose name suggests numeric data
//! but whose declaration block lacks `font-variant-numeric: tabular-nums`.
//!
//! tree-sitter-css gives us a structured `rule_set { selectors block }`,
//! but the `font-variant-numeric: tabular-nums` declaration is recognised
//! as plain `value` text — so we keep the check at the rule_set level and
//! search the block text for the directive.

use crate::diagnostic::{Diagnostic, Severity};

const DATA_HINTS: &[&str] = &[
    "counter",
    "count",
    "price",
    "amount",
    "metric",
    "stat",
    "number",
    "value-display",
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
        path: std::sync::Arc::clone(&ctx.path_arc),
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn flags_counter_selector_without_directive() {
        let src = ".counter { color: red; }";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.css").len(), 1);
    }

    #[test]
    fn allows_counter_with_tabular_nums() {
        let src = ".counter { font-variant-numeric: tabular-nums; }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.css").is_empty());
    }

    #[test]
    fn ignores_unrelated_selector() {
        let src = ".card { color: red; }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.css").is_empty());
    }
}
