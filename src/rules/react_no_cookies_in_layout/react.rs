//! react-no-cookies-in-layout AST backend.
//!
//! Detects `cookies()` or `headers()` calls in files named `layout.*`.
//! In Next.js App Router, these functions opt the route into dynamic
//! rendering. When called from a layout, the blast radius is the entire
//! route segment: every child page becomes dynamic.

use crate::diagnostic::{Diagnostic, Severity};

const DYNAMIC_FNS: &[&str] = &["cookies", "headers"];

/// Returns `true` if `source` contains an import from `next/headers`.
fn has_next_headers_import(source: &[u8]) -> bool {
    let s = std::str::from_utf8(source).unwrap_or("");
    s.contains("from 'next/headers'") || s.contains("from \"next/headers\"")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Only fire on files named `layout.*`.
    let file_stem = ctx.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if file_stem != "layout" {
        return;
    }

    // Only flag when the dynamic functions come from next/headers.
    if !has_next_headers_import(source) {
        return;
    }

    // Match call_expression nodes.
    let Some(callee) = node.child_by_field_name("function") else { return };
    let Ok(callee_text) = callee.utf8_text(source) else { return };

    if DYNAMIC_FNS.contains(&callee_text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-cookies-in-layout".into(),
            message: format!(
                "`{callee_text}()` in a layout file forces EVERY child page to \
                 be dynamically rendered. Move it to the individual page \
                 that needs it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run_with_path(source: &str, path: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .expect("grammar should load");
        let tree = parser.parse(source, None).expect("parser should produce a tree");
        use crate::rules::backend::AstCheck;
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_cookies_in_layout() {
        let src = "import { cookies } from 'next/headers';\nconst c = cookies();\nexport default function Layout() { return <div />; }";
        assert_eq!(run_with_path(src, "app/layout.tsx").len(), 1);
    }

    #[test]
    fn flags_headers_in_layout() {
        let src = "import { headers } from \"next/headers\";\nconst h = headers();\nexport default function Layout() { return <div />; }";
        assert_eq!(run_with_path(src, "app/layout.tsx").len(), 1);
    }

    #[test]
    fn allows_cookies_in_page() {
        let src = "import { cookies } from 'next/headers';\nconst c = cookies();\nexport default function Page() { return <div />; }";
        assert!(run_with_path(src, "app/page.tsx").is_empty());
    }

    #[test]
    fn allows_layout_without_dynamic_calls() {
        let src = "export default function Layout() { return <div />; }";
        assert!(run_with_path(src, "app/layout.tsx").is_empty());
    }

    #[test]
    fn allows_local_cookies_without_next_headers_import() {
        let src = "function cookies() { return {}; }\nconst c = cookies();\nexport default function Layout() { return <div />; }";
        assert!(run_with_path(src, "app/layout.tsx").is_empty());
    }
}
