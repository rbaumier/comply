//! no-unknown-property backend — flag invalid HTML-style attributes on
//! intrinsic JSX elements (lowercase tags).
//!
//! We walk `jsx_opening_element` and `jsx_self_closing_element` nodes,
//! skip anything whose tag name does not start with a lowercase letter
//! (PascalCase custom components take whatever props they want), then
//! inspect every `jsx_attribute` child. The attribute name is looked up
//! in a static HTML → React map; lowercase `on[a-z]+` event handlers
//! fall back to a camelCase suggestion.
//!
//! `data-*`, `aria-*`, attributes containing a `-` already (kebab-case),
//! and any name that already contains uppercase letters are ignored.

use crate::diagnostic::{Diagnostic, Severity};

/// Static map of known HTML attribute name → React camelCase equivalent.
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

/// Look up the React equivalent for an HTML attribute name, or infer
/// one for lowercase `on[a-z]+` event handlers.
fn react_equivalent(name: &str) -> Option<String> {
    if let Some((_, react)) = HTML_TO_REACT.iter().find(|(html, _)| *html == name) {
        return Some((*react).to_string());
    }
    // Lowercase event handler: `onclick` → `onClick`, `ondblclick` → `onDblClick`.
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

/// An intrinsic HTML/SVG tag is a JSX element whose tag name starts
/// with a lowercase ASCII letter. Everything else (PascalCase, dotted
/// access like `Foo.Bar`, namespaced tags) is treated as user code and
/// skipped.
fn is_intrinsic_tag(tag: &str) -> bool {
    tag.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };
    if !is_intrinsic_tag(tag) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };

        // Skip namespaced / data / aria attributes — they're pass-through.
        if attr_name.contains('-') || attr_name.contains(':') {
            continue;
        }
        // If it already has any uppercase letter, it's camelCase — trust it.
        if attr_name.chars().any(|c| c.is_ascii_uppercase()) {
            continue;
        }

        let Some(suggested) = react_equivalent(attr_name) else { continue };

        let pos = child.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: format!(
                "unknown JSX prop `{attr_name}` on `<{tag}>` — use `{suggested}` (React uses camelCase prop names)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
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
