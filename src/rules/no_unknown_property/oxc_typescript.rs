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

            if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
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

    /// Run the check against `source` placed at `importer_rel`, with a
    /// `tsconfig.json` written at `tsconfig_rel`, both under a fresh temp dir.
    /// Lets a test exercise the on-disk tsconfig lookup the rule performs.
    fn run_with_tsconfig(
        importer_rel: &str,
        source: &str,
        tsconfig_rel: &str,
        tsconfig: &str,
    ) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let ts_path = dir.path().join(tsconfig_rel);
        fs::create_dir_all(ts_path.parent().unwrap()).unwrap();
        fs::write(&ts_path, tsconfig).unwrap();
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        let canon = fs::canonicalize(&importer).unwrap();
        let source_file = SourceFile {
            path: canon.clone(),
            language: Language::from_path(&canon).unwrap(),
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
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
    fn flags_class_in_react_file_importing_react() {
        // A React file (imports `react`, no Solid signal) must still be flagged
        // with the `className` suggestion. (Closes #1244)
        let src = "import { useState } from 'react';\nconst a = <div class=\"x\" />;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("className"));
    }

    #[test]
    fn allows_class_in_solidstart_route_without_solid_js_import() {
        // SolidStart routes often import only from the `@solidjs/*` ecosystem
        // (meta/router/start) and never from `solid-js` itself, yet use the
        // native `class` attribute. They must not be flagged. (Closes #1244)
        let src = "import { Title } from '@solidjs/meta';\nconst a = <div class=\"description\" />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_charset_in_solidstart_entry_server() {
        // SolidStart's `entry-server.tsx` imports only from `@solidjs/start/server`
        // (never from `solid-js` directly) yet renders the document shell with
        // native HTML attribute names. `<meta charset="utf-8" />` is correct Solid
        // JSX and must not be flagged with the React `charSet` suggestion. (Closes #2212)
        let src = "import { createHandler, StartServer } from '@solidjs/start/server';\n\
                   const a = <meta charset=\"utf-8\" />;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn allows_class_in_tanstack_solid_router_route() {
        // TanStack Router's Solid adapter (`@tanstack/solid-router`) produces
        // Solid JSX — `class` is the native Solid attribute. Routes often import
        // only from the adapter and never from `solid-js` itself, yet must not be
        // flagged. (Closes #2145)
        let src = "import { createFileRoute } from '@tanstack/solid-router';\n\
                   const a = <div class=\"p-2\" />;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn flags_class_in_tanstack_react_router_route() {
        // The React adapter (`@tanstack/react-router`) produces React JSX —
        // `class` must still be flagged with the `className` suggestion. The
        // exemption requires the Solid adapter source, not any `@tanstack/*-router`.
        let src = "import { createFileRoute } from '@tanstack/react-router';\n\
                   const a = <div class=\"x\" />;";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("className"));
    }

    #[test]
    fn flags_for_in_react_jsx() {
        let src = "const a = <label for=\"x\" />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_class_in_qwik_tsconfig_jsx_import_source() {
        // A Qwik file with no `@builder.io/qwik` import — the JSX factory comes
        // solely from the package tsconfig's `compilerOptions.jsxImportSource`.
        // `class` is correct here and must not be flagged. (Closes #2235)
        let diags = run_with_tsconfig(
            "src/repl-options.tsx",
            "export const X = () => <div class=\"x\" />;",
            "tsconfig.json",
            r#"{"compilerOptions":{"jsx":"react-jsx","jsxImportSource":"@builder.io/qwik"}}"#,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn flags_class_in_react_file_with_react_jsx_import_source() {
        // A real React project whose tsconfig sets `jsxImportSource: "react"`
        // (or omits it) still gets the `className` suggestion.
        let diags = run_with_tsconfig(
            "src/app.tsx",
            "export const X = () => <div class=\"x\" />;",
            "tsconfig.json",
            r#"{"compilerOptions":{"jsx":"react-jsx","jsxImportSource":"react"}}"#,
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("className"));
    }

    #[test]
    fn flags_class_in_react_file_with_no_jsx_import_source() {
        // tsconfig with no `jsxImportSource` at all — plain React, still flagged.
        let diags = run_with_tsconfig(
            "src/app.tsx",
            "export const X = () => <div class=\"x\" />;",
            "tsconfig.json",
            r#"{"compilerOptions":{"jsx":"react-jsx"}}"#,
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("className"));
    }

    #[test]
    fn allows_class_with_non_react_jsx_import_source_pragma() {
        // A file with `@jsxImportSource` pointing at a non-React dialect (here a
        // relative source, as Hono's runtime-tests use) intentionally uses native
        // HTML attribute names — `class`, `tabindex` must not be flagged. (Closes #2103)
        let src = "/** @jsxImportSource ../../src/jsx */\n\
                   const a = <div class={x} tabindex={0} />;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn allows_class_with_hono_jsx_import_source_pragma() {
        let src = "/** @jsxImportSource hono/jsx */\n\
                   const a = <h1 class='foo'>hello</h1>;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn flags_class_with_react_jsx_import_source_pragma() {
        // A `@jsxImportSource react` pragma still names React — the file keeps the
        // `className` suggestion. The exemption requires a *non-React* source.
        let src = "/** @jsxImportSource react */\n\
                   const a = <div class=\"x\" />;";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("className"));
    }

    #[test]
    fn flags_class_with_react_dom_jsx_import_source_pragma() {
        let src = "/** @jsxImportSource react-dom */\n\
                   const a = <div class=\"x\" />;";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("className"));
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
