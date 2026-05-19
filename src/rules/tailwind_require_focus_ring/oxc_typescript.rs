//! tailwind-require-focus-ring oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression, JSXAttributeItem, JSXExpression};
use oxc_span::GetSpan;
use std::sync::Arc;

const INTERACTIVE_TAGS: &[&str] = &["button", "a", "input", "select", "textarea"];
const CLASS_COMPOSERS: &[&str] = &["cn", "clsx", "classnames", "twMerge"];

/// `buttonVariants(...)`, `cva(...)`, or any `cn(...)` / `clsx(...)` / `twMerge(...)`
/// whose arguments contain such a call. Convention is strong: any identifier ending
/// in `Variants` is treated as a cva factory.
fn is_cva_call(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            name == "cva" || name.ends_with("Variants")
        }
        Expression::StaticMemberExpression(member) => {
            let name = member.property.name.as_str();
            name == "cva" || name.ends_with("Variants")
        }
        _ => false,
    }
}

fn is_class_composer_wrapping_cva(call: &CallExpression) -> bool {
    let composer_name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return false,
    };
    if !CLASS_COMPOSERS.contains(&composer_name) {
        return false;
    }
    call.arguments.iter().any(|arg| {
        let Some(Expression::CallExpression(inner)) = arg.as_expression() else {
            return false;
        };
        is_cva_call(inner) || is_class_composer_wrapping_cva(inner)
    })
}

fn class_name_is_cva_driven(expr: &JSXExpression) -> bool {
    let JSXExpression::CallExpression(call) = expr else {
        return false;
    };
    is_cva_call(call) || is_class_composer_wrapping_cva(call)
}

fn has_focus_ring(classes: &str) -> bool {
    const OUTLINE_REMOVERS: &[&str] = &[
        "focus:outline-none",
        "focus:outline-0",
        "focus-visible:outline-none",
        "focus-visible:outline-0",
    ];
    classes.split_whitespace().any(|tok| {
        if OUTLINE_REMOVERS.contains(&tok) {
            return false;
        }
        tok.starts_with("focus:ring")
            || tok.starts_with("focus-visible:ring")
            || tok.starts_with("focus:outline")
            || tok.starts_with("focus-visible:outline")
            || tok.starts_with("focus:border-")
            || tok.starts_with("focus-visible:border-")
    })
}

pub struct Check;

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

        // shadcn/ui primitives handle focus indicators internally.
        let path_str = ctx.path.to_str().unwrap_or("");
        if path_str.contains("/components/ui/") || path_str.contains("/lib/ui/") {
            return;
        }

        let tag = &ctx.source[opening.name.span().start as usize..opening.name.span().end as usize];
        // PascalCase = React component — focus ring may be baked in.
        if tag.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase()) {
            return;
        }
        let lower = tag.to_ascii_lowercase();

        let mut class_value: Option<&str> = None;
        let mut is_role_button = false;
        let mut class_name_exempt = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
                continue;
            };
            let name = ident.name.as_str();
            match name {
                "className" | "class" => match &attr.value {
                    Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) => {
                        class_value = Some(lit.value.as_str());
                    }
                    Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(ec))
                        if class_name_is_cva_driven(&ec.expression) =>
                    {
                        class_name_exempt = true;
                    }
                    _ => {}
                },
                "role" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value
                        && lit.value.as_str() == "button" {
                            is_role_button = true;
                        }
                }
                _ => {}
            }
        }

        let interactive = INTERACTIVE_TAGS.contains(&lower.as_str()) || is_role_button;
        if !interactive {
            return;
        }

        if class_name_exempt {
            return;
        }

        let classes = class_value.unwrap_or("");
        if has_focus_ring(classes) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Interactive element missing a `focus:ring-*` class — keyboard users need a visible focus indicator.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    #[test]
    fn flags_button_without_focus_ring() {
        assert_eq!(
            run(r#"export const A = () => <button className="px-4" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_role_button_without_focus_ring() {
        assert_eq!(
            run(r#"export const A = () => <div role="button" className="px-4" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_button_with_focus_ring() {
        assert!(
            run(r#"export const A = () => <button className="px-4 focus:ring-2" />;"#).is_empty()
        );
    }

    #[test]
    fn allows_input_with_focus_visible_ring() {
        assert!(
            run(r#"export const A = () => <input className="focus-visible:ring-2" />;"#).is_empty()
        );
    }

    #[test]
    fn ignores_non_interactive_div() {
        assert!(run(r#"export const A = () => <div className="px-4" />;"#).is_empty());
    }

    #[test]
    fn flags_focus_outline_none_alone() {
        assert_eq!(
            run(r#"export const A = () => <button className="focus:outline-none" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_outline_none_paired_with_ring() {
        assert!(
            run(
                r#"export const A = () => <button className="focus:outline-none focus:ring-2" />;"#
            )
            .is_empty()
        );
    }

    #[test]
    fn skips_pascal_case_components() {
        assert!(run(r#"export const A = () => <Button className="px-4" />;"#).is_empty());
    }

    #[test]
    fn skips_button_with_button_variants_call() {
        assert!(
            run(r#"export const A = () => <button className={buttonVariants({ variant: "outline" })}>x</button>;"#)
                .is_empty()
        );
    }

    #[test]
    fn skips_anchor_with_link_variants_call() {
        assert!(
            run(r#"export const A = () => <a className={linkVariants({ variant: "default" })}>x</a>;"#)
                .is_empty()
        );
    }

    #[test]
    fn skips_button_with_cn_wrapping_button_variants() {
        assert!(
            run(r#"export const A = () => <button className={cn(buttonVariants({ variant: "outline" }), "extra")}>x</button>;"#)
                .is_empty()
        );
    }

    #[test]
    fn skips_button_with_clsx_wrapping_button_variants() {
        assert!(
            run(r#"export const A = () => <button className={clsx("extra", buttonVariants({ variant: "outline" }))}>x</button>;"#)
                .is_empty()
        );
    }

    #[test]
    fn still_flags_button_with_plain_string_no_focus() {
        assert_eq!(
            run(r#"export const A = () => <button className="bg-blue-500 text-white" />;"#).len(),
            1
        );
    }

    #[test]
    fn still_flags_button_with_cn_no_cva() {
        assert_eq!(
            run(r#"export const A = () => <button className={cn("bg-blue-500", "text-white")} />;"#)
                .len(),
            1
        );
    }

    #[test]
    fn skips_shadcn_ui_components() {
        let src = r#"export const A = <button className="px-4" />;"#;
        let d = crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "src/components/ui/sidebar.tsx");
        assert!(d.is_empty());
    }
}
