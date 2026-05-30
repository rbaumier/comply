//! elysia-html-xss-no-safe OxcCheck backend.
//!
//! Flag JSX elements that interpolate user input (body, query, params)
//! without a `safe` attribute on the surrounding element.
//! Only applies to files that import from `@elysiajs/html` or `elysia/html`;
//! React JSX files are skipped because React escapes string interpolations by default.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXChild};
use std::sync::Arc;

pub struct Check;

fn file_imports_elysia_html(source: &str) -> bool {
    source.contains("from '@elysiajs/html'")
        || source.contains("from \"@elysiajs/html\"")
        || source.contains("from 'elysia/html'")
        || source.contains("from \"elysia/html\"")
}

/// Returns true when `text` contains `body`, `query`, or `params` as a
/// standalone identifier.
fn mentions_dangerous_identifier(text: &str) -> bool {
    const NAMES: &[&str] = &["body", "query", "params"];
    let bytes = text.as_bytes();
    for name in NAMES {
        let nb = name.as_bytes();
        let mut i = 0;
        while i + nb.len() <= bytes.len() {
            if &bytes[i..i + nb.len()] == nb {
                let before_ok =
                    i == 0 || matches!(bytes[i - 1], b'{' | b'.' | b' ' | b'\t' | b'\n' | b'\r');
                let after_idx = i + nb.len();
                let after_ok = after_idx == bytes.len()
                    || matches!(
                        bytes[after_idx],
                        b'.' | b'}' | b'[' | b',' | b' ' | b'\t' | b'\n' | b'\r'
                    );
                if before_ok && after_ok {
                    return true;
                }
            }
            i += 1;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !file_imports_elysia_html(ctx.source) {
            return;
        }

        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // Walk up to parent JSXElement to inspect children.
        let Some(parent) = semantic.nodes().ancestors(node.id()).next() else {
            return;
        };
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        // Check if any direct JSXExpressionContainer child mentions dangerous identifiers.
        let mut has_dangerous_expr = false;
        for child in &element.children {
            if let JSXChild::ExpressionContainer(container) = child {
                let start = container.span.start as usize;
                let end = container.span.end as usize;
                if end <= ctx.source.len() {
                    let expr_text = &ctx.source[start..end];
                    if mentions_dangerous_identifier(expr_text) {
                        has_dangerous_expr = true;
                        break;
                    }
                }
            }
        }
        if !has_dangerous_expr {
            return;
        }

        // Check for `safe` attribute on the opening element.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() == "safe" {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, element.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "JSX element interpolates user input without `safe` \u{2014} add the `safe` attribute to escape it.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_elysia_html_jsx_with_body_no_safe() {
        let src = "import { html } from '@elysiajs/html';\nconst v = <div>{body.name}</div>;";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_elysia_html_jsx_with_safe() {
        let src = "import { html } from '@elysiajs/html';\nconst v = <div safe>{body.name}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_elysia_html_slash_import() {
        let src = "import { html } from 'elysia/html';\nconst v = <div safe>{body.name}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_react_jsx_with_body_prop() {
        // React JSX in an Elysia project — file imports from 'react', not '@elysiajs/html'.
        // React escapes string interpolations by default, so `safe` is meaningless here.
        // Closes #426.
        let src = "import React from 'react';\nfunction ErrorScreen({ body }: { body: string }) {\n  return <div>{body}</div>;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_react_jsx_with_query_prop() {
        let src = "import React from 'react';\nfunction Search({ query }: { query: string }) {\n  return <p>{query}</p>;\n}";
        assert!(run_on(src).is_empty());
    }
}
