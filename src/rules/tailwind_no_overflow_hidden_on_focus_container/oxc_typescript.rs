//! tailwind-no-overflow-hidden-on-focus-container oxc backend for TS / JS / TSX.
//!
//! Fires on a JSX element whose `className`/`class` contains the
//! `overflow-hidden` token *only* when its JSX subtree holds a statically
//! focusable descendant whose focus ring the clip would cut off. A container
//! whose descendants are all non-focusable intrinsics (`img`, `svg`, `span`,
//! `<Text>`, plain text, …) or custom components of unknown focusability is left
//! alone — precision over recall: missing the rare focusable custom component is
//! the accepted trade-off to kill the image-crop / text-truncation FP class.

use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElementName, JSXExpression,
    UnaryOperator,
};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};

const TARGET: &str = "overflow-hidden";

/// Intrinsic elements that are focusable on their own (no extra attribute
/// required). `a` is deliberately absent — a bare `<a>` without `href` is not
/// focusable, so it is handled separately in [`is_focusable_opening`].
const FOCUSABLE_ELEMENTS: &[&str] =
    &["button", "input", "select", "textarea", "summary"];

/// ARIA roles that make any element keyboard-focusable and thus a focus-ring
/// host. Mirrors the interactive-role set used by `html-no-nested-interactive`.
const FOCUSABLE_ROLES: &[&str] = &[
    "button",
    "link",
    "checkbox",
    "radio",
    "switch",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "combobox",
    "listbox",
    "slider",
    "spinbutton",
    "textbox",
    "searchbox",
    "treeitem",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[TARGET])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };
        if !opening_has_overflow_hidden(&element.opening_element.attributes) {
            return;
        }
        if !find_focusable_descendant(&element.children) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, element.opening_element.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`overflow-hidden` on a container with a focusable child clips its \
                      focus ring — use `overflow-clip` or move clipping to a wrapper \
                      without focusable children."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Whether the element's `className`/`class` attribute carries `overflow-hidden`
/// as a whole class token. Reads static string literals and the leading static
/// string of a template literal (`` `overflow-hidden ${x}` ``); a fully dynamic
/// class expression is treated as not carrying the token.
fn opening_has_overflow_hidden(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>) -> bool {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        let n = name.name.as_str();
        if n != "className" && n != "class" {
            continue;
        }
        match &attr.value {
            Some(JSXAttributeValue::StringLiteral(lit)) => {
                return class_str_has_target(lit.value.as_str());
            }
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                return expression_has_target(&container.expression);
            }
            _ => return false,
        }
    }
    false
}

/// Best-effort scan of a class expression for `overflow-hidden`: handles a plain
/// string literal, a template literal's static quasis, and `clsx("…", …)`-style
/// call arguments / conditional branches by recursing through string and
/// template parts. Identifier-only / fully dynamic parts are skipped.
fn expression_has_target(expr: &JSXExpression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        JSXExpression::StringLiteral(lit) => class_str_has_target(lit.value.as_str()),
        JSXExpression::TemplateLiteral(tpl) => tpl
            .quasis
            .iter()
            .any(|q| class_str_has_target(q.value.raw.as_str())),
        JSXExpression::CallExpression(call) => call
            .arguments
            .iter()
            .filter_map(|arg| arg.as_expression())
            .any(any_expr_has_target),
        JSXExpression::ConditionalExpression(cond) => {
            any_expr_has_target(&cond.consequent) || any_expr_has_target(&cond.alternate)
        }
        _ => false,
    }
}

/// [`expression_has_target`] for a plain `Expression` (call args, conditional
/// branches), so `clsx("base overflow-hidden", cond && "…")` is covered.
fn any_expr_has_target(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::StringLiteral(lit) => class_str_has_target(lit.value.as_str()),
        Expression::TemplateLiteral(tpl) => tpl
            .quasis
            .iter()
            .any(|q| class_str_has_target(q.value.raw.as_str())),
        Expression::CallExpression(call) => call
            .arguments
            .iter()
            .filter_map(|arg| arg.as_expression())
            .any(any_expr_has_target),
        Expression::ConditionalExpression(cond) => {
            any_expr_has_target(&cond.consequent) || any_expr_has_target(&cond.alternate)
        }
        Expression::LogicalExpression(logical) => {
            any_expr_has_target(&logical.left) || any_expr_has_target(&logical.right)
        }
        _ => false,
    }
}

/// Whether `overflow-hidden` appears as a whitespace-delimited class token.
fn class_str_has_target(class_str: &str) -> bool {
    class_str.split_whitespace().any(|tok| tok == TARGET)
}

/// Recursively search the JSX subtree for a statically-focusable descendant.
fn find_focusable_descendant(children: &oxc_allocator::Vec<'_, JSXChild>) -> bool {
    for child in children {
        match child {
            JSXChild::Element(el) => {
                if is_focusable_opening(&el.opening_element) {
                    return true;
                }
                if find_focusable_descendant(&el.children) {
                    return true;
                }
            }
            JSXChild::Fragment(frag) => {
                if find_focusable_descendant(&frag.children) {
                    return true;
                }
            }
            JSXChild::ExpressionContainer(_) | JSXChild::Spread(_) | JSXChild::Text(_) => {}
        }
    }
    false
}

/// Whether the opening tag is a statically-focusable element: a focusable
/// intrinsic tag, an `<a href>`, an element with a focusable `role`, or one with
/// a non-negative `tabIndex`. A custom component (PascalCase / member name) is of
/// unknown focusability and is treated as non-focusable.
fn is_focusable_opening(opening: &oxc_ast::ast::JSXOpeningElement) -> bool {
    if let Some(tag) = intrinsic_tag(&opening.name) {
        let lower = tag.to_ascii_lowercase();
        if FOCUSABLE_ELEMENTS.contains(&lower.as_str()) {
            return true;
        }
        if lower == "a" && has_href(&opening.attributes) {
            return true;
        }
    }
    has_focusable_role(&opening.attributes) || has_focusable_tabindex(&opening.attributes)
}

/// The tag name only when it is an intrinsic host element (lowercase identifier).
/// React treats a lowercase identifier as a DOM tag and a capitalized one as a
/// component; member/namespaced names are components too. Returns `None` for any
/// non-intrinsic name so custom components never count as focusable by tag.
fn intrinsic_tag<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    let JSXElementName::Identifier(id) = name else {
        return None;
    };
    let s = id.name.as_str();
    let first_is_lower = s.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    first_is_lower.then_some(s)
}

fn has_href(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>) -> bool {
    attrs.iter().any(|item| {
        let JSXAttributeItem::Attribute(attr) = item else {
            return false;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            return false;
        };
        name.name.as_str() == "href"
    })
}

fn has_focusable_role(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>) -> bool {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        if name.name.as_str() != "role" {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
            return lit
                .value
                .split_whitespace()
                .any(|r| FOCUSABLE_ROLES.contains(&r));
        }
    }
    false
}

/// Whether a `tabIndex`/`tabindex` attribute makes the element focusable
/// (any value other than `-1`). A dynamic value is conservatively treated as
/// focusable, except an explicit `-1` literal.
fn has_focusable_tabindex(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>) -> bool {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        let n = name.name.as_str();
        if n != "tabIndex" && n != "tabindex" {
            continue;
        }
        return match &attr.value {
            Some(JSXAttributeValue::StringLiteral(lit)) => lit.value.as_str() != "-1",
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                match &container.expression {
                    JSXExpression::NumericLiteral(num) => num.value != -1.0,
                    JSXExpression::UnaryExpression(unary) => {
                        !(unary.operator == UnaryOperator::UnaryNegation
                            && matches!(
                                &unary.argument,
                                oxc_ast::ast::Expression::NumericLiteral(num) if num.value == 1.0
                            ))
                    }
                    _ => true,
                }
            }
            _ => false,
        };
    }
    false
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // ---- issue #4683: non-focusable-only containers must NOT fire ----

    #[test]
    fn allows_thumbnail_with_only_img_and_svg_component() {
        // medusajs/medusa thumbnail.tsx — `overflow-hidden` rounds an image; the
        // only children are a non-focusable <img> and a custom SVG component.
        let src = r#"const Thumbnail = ({ thumbnail, alt = "" }) => (
            <div className="relative w-6 h-8 rounded overflow-hidden flex items-center justify-center bg-ui-bg-component">
              {thumbnail ? (
                <img src={thumbnail} className="w-full h-full object-cover" alt={alt} />
              ) : (
                <Photo />
              )}
            </div>
        );"#;
        assert!(
            run(src).is_empty(),
            "image-crop container with only <img>/<Photo/> must not fire"
        );
    }

    #[test]
    fn allows_text_truncation_container() {
        // user-menu.tsx — `overflow-hidden` + truncate is the CSS text-overflow
        // technique; <Text> is a non-focusable custom component.
        let src = r#"const x = (
            <div className="flex items-center overflow-hidden">
              <Text size="xsmall" weight="plus" className="truncate">{displayName}</Text>
            </div>
        );"#;
        assert!(run(src).is_empty(), "text-truncation container must not fire");
    }

    #[test]
    fn allows_overflow_hidden_with_only_plain_intrinsics() {
        assert!(
            run(r#"const x = <div className="overflow-hidden"><span><p>hi</p></span></div>;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_overflow_hidden_with_no_children() {
        assert!(run(r#"const x = <div className="overflow-hidden" />;"#).is_empty());
    }

    #[test]
    fn allows_overflow_hidden_with_only_custom_components() {
        // Custom components are of unknown focusability — precision over recall.
        assert!(
            run(r#"const x = <div className="overflow-hidden"><Avatar /><Badge /></div>;"#)
                .is_empty()
        );
    }

    // ---- positive: focusable descendant still flags ----

    #[test]
    fn flags_overflow_hidden_with_button_child() {
        assert_eq!(
            run(r#"const x = <div className="overflow-hidden"><button>Go</button></div>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_overflow_hidden_with_nested_button() {
        assert_eq!(
            run(r#"const x = <div className="rounded overflow-hidden"><span><button>x</button></span></div>;"#)
                .len(),
            1
        );
    }

    #[test]
    fn flags_overflow_hidden_with_anchor_href() {
        assert_eq!(
            run(r#"const x = <div className="overflow-hidden"><a href="/x">link</a></div>;"#).len(),
            1
        );
    }

    #[test]
    fn allows_overflow_hidden_with_anchor_without_href() {
        // A bare <a> without href is not focusable.
        assert!(
            run(r#"const x = <div className="overflow-hidden"><a>label</a></div>;"#).is_empty()
        );
    }

    #[test]
    fn flags_overflow_hidden_with_tabindex_descendant() {
        assert_eq!(
            run(r#"const x = <div className="overflow-hidden"><div tabIndex={0}>x</div></div>;"#)
                .len(),
            1
        );
    }

    #[test]
    fn allows_overflow_hidden_with_tabindex_negative_one() {
        assert!(
            run(r#"const x = <div className="overflow-hidden"><div tabIndex={-1}>x</div></div>;"#)
                .is_empty()
        );
    }

    #[test]
    fn flags_overflow_hidden_with_role_button_descendant() {
        assert_eq!(
            run(r#"const x = <div className="overflow-hidden"><div role="button">x</div></div>;"#)
                .len(),
            1
        );
    }

    #[test]
    fn flags_overflow_hidden_with_input_descendant() {
        assert_eq!(
            run(r#"const x = <div className="overflow-hidden"><label><input /></label></div>;"#)
                .len(),
            1
        );
    }

    // ---- other-overflow utilities are untouched ----

    #[test]
    fn allows_overflow_clip_with_button() {
        assert!(
            run(r#"const x = <div className="overflow-clip"><button>x</button></div>;"#).is_empty()
        );
    }

    #[test]
    fn allows_overflow_auto_with_button() {
        assert!(
            run(r#"const x = <div className="overflow-auto"><button>x</button></div>;"#).is_empty()
        );
    }

    // ---- className shapes ----

    #[test]
    fn flags_template_literal_classname() {
        assert_eq!(
            run(r#"const x = <div className={`base overflow-hidden ${dyn}`}><button>x</button></div>;"#)
                .len(),
            1
        );
    }

    #[test]
    fn flags_clsx_classname() {
        assert_eq!(
            run(r#"const x = <div className={clsx("base overflow-hidden", cond && "p-2")}><button>x</button></div>;"#)
                .len(),
            1
        );
    }

    #[test]
    fn allows_substring_not_whole_token() {
        // `no-overflow-hidden` is not the `overflow-hidden` utility.
        assert!(
            run(r#"const x = <div className="no-overflow-hidden-x"><button>y</button></div>;"#)
                .is_empty()
        );
    }
}
