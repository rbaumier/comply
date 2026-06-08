//! OXC backend for no-unknown-property.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Static map of known HTML attribute name -> React camelCase equivalent.
const HTML_TO_REACT: &[(&str, &str)] = &[
    ("class", "className"),
    ("for", "htmlFor"),
    ("tabindex", "tabIndex"),
    ("autofocus", "autoFocus"),
    ("readonly", "readOnly"),
    ("maxlength", "maxLength"),
    ("minlength", "minLength"),
    ("colspan", "colSpan"),
    ("rowspan", "rowSpan"),
    ("cellpadding", "cellPadding"),
    ("cellspacing", "cellSpacing"),
    ("charset", "charSet"),
    ("crossorigin", "crossOrigin"),
    ("formaction", "formAction"),
    ("formenctype", "formEncType"),
    ("formmethod", "formMethod"),
    ("formnovalidate", "formNoValidate"),
    ("formtarget", "formTarget"),
    ("frameborder", "frameBorder"),
    ("hreflang", "hrefLang"),
    ("httpequiv", "httpEquiv"),
    ("inputmode", "inputMode"),
    ("nomodule", "noModule"),
    ("novalidate", "noValidate"),
    ("srcset", "srcSet"),
    ("srcdoc", "srcDoc"),
    ("srclang", "srcLang"),
    ("usemap", "useMap"),
    ("accesskey", "accessKey"),
    ("autocomplete", "autoComplete"),
    ("enctype", "encType"),
    ("contenteditable", "contentEditable"),
    ("spellcheck", "spellCheck"),
    ("allowfullscreen", "allowFullScreen"),
    ("autoplay", "autoPlay"),
    ("playsinline", "playsInline"),
    ("datetime", "dateTime"),
];

fn react_equivalent(name: &str) -> Option<String> {
    if let Some((_, react)) = HTML_TO_REACT.iter().find(|(html, _)| *html == name) {
        return Some((*react).to_string());
    }
    // Lowercase event handler: `onclick` -> `onClick`
    if let Some(rest) = name.strip_prefix("on")
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_lowercase())
    {
        let mut out = String::from("on");
        let mut chars = rest.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
        }
        out.extend(chars);
        return Some(out);
    }
    None
}

fn is_intrinsic_tag(tag: &str) -> bool {
    tag.chars().next().is_some_and(|c| c.is_ascii_lowercase())
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

        // Get the tag name
        let tag = jsx_element_name(&opening.name);
        if !is_intrinsic_tag(&tag) {
            return;
        }

        for attr in &opening.attributes {
            let oxc_ast::ast::JSXAttributeItem::Attribute(attr) = attr else {
                continue;
            };

            let attr_name = jsx_attr_name(&attr.name);

            // Skip namespaced / data / aria attributes
            if attr_name.contains('-') || attr_name.contains(':') {
                continue;
            }
            // If it already has any uppercase letter, trust it
            if attr_name.chars().any(|c| c.is_ascii_uppercase()) {
                continue;
            }

            let Some(suggested) = react_equivalent(&attr_name) else {
                continue;
            };

            let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "unknown JSX prop `{attr_name}` on `<{tag}>` — use `{suggested}` (React uses camelCase prop names)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn jsx_element_name(name: &oxc_ast::ast::JSXElementName) -> String {
    match name {
        oxc_ast::ast::JSXElementName::Identifier(id) => id.name.to_string(),
        oxc_ast::ast::JSXElementName::IdentifierReference(id) => id.name.to_string(),
        oxc_ast::ast::JSXElementName::NamespacedName(ns) => {
            format!("{}:{}", ns.namespace.name, ns.name.name)
        }
        oxc_ast::ast::JSXElementName::MemberExpression(member) => {
            jsx_member_expr_name(member)
        }
        _ => String::new(),
    }
}

fn jsx_member_expr_name(member: &oxc_ast::ast::JSXMemberExpression) -> String {
    let obj = match &member.object {
        oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
        oxc_ast::ast::JSXMemberExpressionObject::MemberExpression(m) => jsx_member_expr_name(m),
        _ => String::new(),
    };
    format!("{}.{}", obj, member.property.name)
}

fn jsx_attr_name(name: &oxc_ast::ast::JSXAttributeName) -> String {
    match name {
        oxc_ast::ast::JSXAttributeName::Identifier(id) => id.name.to_string(),
        oxc_ast::ast::JSXAttributeName::NamespacedName(ns) => {
            format!("{}:{}", ns.namespace.name, ns.name.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }


    #[test]
    fn flags_class_attribute() {
        let d = run_on(r#"const x = <div class="foo" />;"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unknown-property");
        assert!(d[0].message.contains("className"));
    }


    #[test]
    fn flags_for_on_label() {
        let d = run_on(r#"const x = <label for="x" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("htmlFor"));
    }


    #[test]
    fn flags_tabindex() {
        let d = run_on(r#"const x = <div tabindex="0" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("tabIndex"));
    }


    #[test]
    fn flags_autofocus_boolean_prop() {
        let d = run_on("const x = <input autofocus />;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("autoFocus"));
    }


    #[test]
    fn flags_colspan() {
        let d = run_on(r#"const x = <td colspan="2" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("colSpan"));
    }


    #[test]
    fn flags_lowercase_event_handler() {
        let d = run_on("const x = <div onclick={() => {}} />;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onClick"));
    }


    #[test]
    fn flags_lowercase_onchange() {
        let d = run_on("const x = <input onchange={() => {}} />;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onChange"));
    }


    #[test]
    fn allows_class_name() {
        assert!(run_on(r#"const x = <div className="foo" />;"#).is_empty());
    }


    #[test]
    fn allows_html_for() {
        assert!(run_on(r#"const x = <label htmlFor="x" />;"#).is_empty());
    }


    #[test]
    fn allows_data_attribute() {
        assert!(run_on(r#"const x = <div data-testid="x" />;"#).is_empty());
    }


    #[test]
    fn allows_aria_attribute() {
        assert!(run_on(r#"const x = <div aria-label="x" />;"#).is_empty());
    }


    #[test]
    fn allows_custom_component_with_unusual_prop() {
        // PascalCase component — we don't know its prop surface, skip entirely.
        assert!(
            run_on(r#"const x = <MyComponent class="foo" for="bar" onclick={f} />;"#).is_empty()
        );
    }


    #[test]
    fn allows_camelcase_event_handler() {
        assert!(run_on("const x = <button onClick={f} />;").is_empty());
    }


    #[test]
    fn allows_standard_react_props() {
        let src = r#"const x = <div style={{ color: 'red' }} key="x" ref={r} />;"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_multiple_bad_props_on_same_element() {
        let d = run_on(r#"const x = <div class="a" tabindex="0" onclick={f} />;"#);
        assert_eq!(d.len(), 3);
    }
}
