//! Detect JSX elements whose className includes numeric-data hints
//! (`counter`, `count`, `price`, `amount`, `metric`, `stat`, `number`)
//! but lack `tabular-nums` / `tabular-numbers`.

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

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    // Walk attributes to find className string.
    let mut class_value: Option<String> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        if name != "className" { continue; }
        if let Some(s) = crate::rules::jsx::jsx_attribute_string_value(child, source) {
            class_value = Some(s.to_string());
        }
    }

    let Some(cls) = class_value else { return };
    let lower = cls.to_ascii_lowercase();
    let has_hint = DATA_HINTS.iter().any(|h| lower.contains(h));
    if !has_hint { return; }
    if lower.contains("tabular-nums") || lower.contains("tabular-numbers") || lower.contains("font-variant-numeric") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "className `{cls}` suggests numeric data but is missing `tabular-nums` — digits will jitter between updates."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_counter_without_tabular_nums() {
        let src = r#"const x = <span className="counter text-lg">42</span>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_price_without_tabular_nums() {
        let src = r#"const x = <div className="price">$9.99</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_counter_with_tabular_nums() {
        let src = r#"const x = <span className="counter tabular-nums">42</span>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_numeric_class() {
        let src = r#"const x = <div className="card hero">hi</div>;"#;
        assert!(run(src).is_empty());
    }
}
