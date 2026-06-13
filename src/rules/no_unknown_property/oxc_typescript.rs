//! OXC backend for no-unknown-property.
//!
//! Files importing from Vue, Solid, Preact, Qwik, or Stencil (or carrying a
//! matching `@jsxImportSource` pragma) are exempt: those frameworks use native
//! HTML attribute names (`class`, `for`, …) in JSX, so React's camelCase prop
//! conventions do not apply.

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

/// True when the file is JSX for a framework that uses native HTML attribute
/// names (`class`, `for`, …) rather than React's camelCase — Vue, Solid,
/// Preact, Qwik, or Stencil — detected via its imports or `@jsxImportSource`
/// pragma. `no-unknown-property` encodes React's prop conventions and must not
/// fire on these.
fn is_non_react_jsx_file(ctx: &CheckCtx) -> bool {
    ctx.source_contains("solid-js")
        || ctx.source_contains("@vue/")
        || ctx.source_contains("@builder.io/qwik")
        || ctx.source_contains("@stencil/core")
        || ctx.source_contains("preact/")
        || ctx.source_contains("'vue'")
        || ctx.source_contains("\"vue\"")
        || ctx.source_contains("'preact'")
        || ctx.source_contains("\"preact\"")
        || ctx.source_contains("jsxImportSource vue")
        || ctx.source_contains("jsxImportSource preact")
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

            if is_non_react_jsx_file(ctx) {
                return;
            }

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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_class_in_react_jsx() {
        let src = "const a = <div class=\"x\" />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_class_in_vue_jsx() {
        let src = "import { ref, defineComponent } from 'vue';\nconst a = <div class=\"x\" />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_class_in_solid_jsx() {
        let src = "import { createSignal } from 'solid-js';\nconst a = <div class=\"x\" />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_for_in_react_jsx() {
        let src = "const a = <label for=\"x\" />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_class_in_stencil_jsx() {
        let src = "import { Component, Host, h } from '@stencil/core';\n\
                   @Component({ tag: 'ion-picker-column-option', shadow: true })\n\
                   export class PickerColumnOption {\n\
                       render() {\n\
                           return (\n\
                               <Host class={createColorClasses(color, { [mode]: true })}>\n\
                                   <div class={'picker-column-option-button'} role=\"button\">\n\
                                       <slot></slot>\n\
                                   </div>\n\
                               </Host>\n\
                           );\n\
                       }\n\
                   }";
        assert!(run(src).is_empty());
    }
}
