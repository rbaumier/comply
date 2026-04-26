//! tailwind-prefer-cn-utility typescript backend — flag `className={...}`
//! whose expression is a bare ternary or string-concatenation. Such shapes
//! resist grep and editor tooling; `cn()` / `clsx()` keep the conditional
//! class logic readable and tree-shakable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_value};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if jsx_attribute_name(node, source) != Some("className") { return; }

    let Some(val) = jsx_attribute_value(node) else { return; };
    if val.kind() != "jsx_expression" { return; }

    let val_text = val.utf8_text(source).unwrap_or("");
    if val_text.contains("cn(") || val_text.contains("clsx(") || val_text.contains("cva(") {
        return;
    }

    // The jsx_expression's first named child is the wrapped expression.
    let Some(inner) = val.named_child(0) else { return; };
    if inner.kind() == "ternary_expression" || inner.kind() == "binary_expression" {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &inner,
            super::META.id,
            "Use `cn()` or `clsx()` for conditional class names instead of ternaries or concatenation.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_ternary_classname() {
        assert_eq!(run(r#"<div className={x ? 'flex' : 'hidden'} />"#).len(), 1);
    }

    #[test]
    fn allows_cn_utility() {
        assert!(run(r#"<div className={cn('p-4', x && 'flex')} />"#).is_empty());
    }

    #[test]
    fn allows_static_classname() {
        assert!(run(r#"<div className="flex p-4" />"#).is_empty());
    }
}
