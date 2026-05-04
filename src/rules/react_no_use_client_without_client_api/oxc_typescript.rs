//! OXC backend for react-no-use-client-without-client-api.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const CLIENT_GLOBALS: &[&str] = &[
    "window",
    "document",
    "navigator",
    "localStorage",
    "sessionStorage",
    "location",
    "history",
    "fetch",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let source = ctx.source;
        if !has_use_client_directive(source) {
            return Vec::new();
        }

        // Scan all nodes (excluding imports) for client API usage
        let mut found_client_api = false;
        for node in semantic.nodes().iter() {
            // Skip import declarations entirely
            if matches!(node.kind(), AstKind::ImportDeclaration(_)) {
                continue;
            }
            // Check if we're inside an import declaration (skip children too)
            let in_import = semantic.nodes().ancestors(node.id()).any(|a| {
                matches!(a.kind(), AstKind::ImportDeclaration(_))
            });
            if in_import {
                continue;
            }

            match node.kind() {
                AstKind::IdentifierReference(id) => {
                    let name = id.name.as_str();
                    if is_client_api_name(name) {
                        found_client_api = true;
                        break;
                    }
                }
                AstKind::IdentifierName(id) => {
                    let name = id.name.as_str();
                    // JSX event handlers: onClick, onMouseMove, etc.
                    if name.starts_with("on")
                        && name.len() > 2
                        && name.as_bytes()[2].is_ascii_uppercase()
                    {
                        found_client_api = true;
                        break;
                    }
                }
                _ => {}
            }
        }

        if found_client_api {
            return Vec::new();
        }

        let (line, column) = byte_offset_to_line_col(source, 0);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`\"use client\"` directive with no hooks, event handlers, or browser APIs — \
                     remove the directive or justify it with client-only behavior."
                .into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

fn has_use_client_directive(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }
        if trimmed == r#""use client";"#
            || trimmed == r#""use client""#
            || trimmed == "'use client';"
            || trimmed == "'use client'"
        {
            return true;
        }
        if trimmed.starts_with("import")
            || trimmed.starts_with("export")
            || trimmed.starts_with("const")
            || trimmed.starts_with("let")
            || trimmed.starts_with("var")
            || trimmed.starts_with("function")
            || trimmed.starts_with("class")
        {
            return false;
        }
    }
    false
}

fn is_client_api_name(name: &str) -> bool {
    if name.starts_with("use") && name.len() > 3 && name.as_bytes()[3].is_ascii_uppercase() {
        return true;
    }
    if name.starts_with("on") && name.len() > 2 && name.as_bytes()[2].is_ascii_uppercase() {
        return true;
    }
    CLIENT_GLOBALS.contains(&name)
}
