//! ui-tabular-nums-on-data OXC backend — flag JSX elements whose className
//! contains numeric-data hints but lacks `tabular-nums`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

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

pub struct Check;

fn extract_class_value<'a>(
    attrs: &'a oxc_allocator::Vec<'a, JSXAttributeItem<'a>>,
) -> Option<&'a str> {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        if name.name.as_str() != "className" {
            continue;
        }
        if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value {
            return Some(lit.value.as_str());
        }
    }
    None
}

fn check_class(
    cls: &str,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let lower = cls.to_ascii_lowercase();
    let has_hint = DATA_HINTS.iter().any(|h| lower.contains(h));
    if !has_hint {
        return;
    }
    if lower.contains("tabular-nums")
        || lower.contains("tabular-numbers")
        || lower.contains("font-variant-numeric")
    {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "className `{cls}` suggests numeric data but is missing `tabular-nums` — digits will jitter between updates."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        if let Some(cls) = extract_class_value(&opening.attributes) {
            check_class(cls, opening.span.start, ctx, diagnostics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
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
