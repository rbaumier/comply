//! AST backend for react-no-use-client-without-client-api.
//!
//! Fires once per `program` when the file begins with a `"use client"`
//! directive but contains no identifier matching a hook (`useX`), a JSX
//! event handler (`on*`), or a browser/global API reference
//! (`window`, `document`, `localStorage`, etc.).

use crate::diagnostic::{Diagnostic, Severity};

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

const CLIENT_ONLY_PACKAGE_PREFIXES: &[&str] = &[
    "@base-ui/react",
    "@radix-ui/",
];

fn has_use_client_directive(program: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        if child.kind() != "expression_statement" {
            // Only leading directives count; first non-directive ends the prologue.
            if child.kind() == "string" || child.kind() == "comment" {
                continue;
            }
            return false;
        }
        let Some(expr) = child.child(0) else {
            return false;
        };
        if expr.kind() != "string" {
            return false;
        }
        let Ok(text) = expr.utf8_text(source) else {
            return false;
        };
        let unquoted = text.trim_matches(|c| c == '"' || c == '\'');
        if unquoted == "use client" {
            return true;
        }
        // Any other directive: keep scanning for "use client".
    }
    false
}

/// Returns true if any import in the program comes from a known client-only package.
fn imports_from_client_only_package(program: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        // The import source is a `string` node — last named child or child named "source"
        let mut inner = child.walk();
        for part in child.children(&mut inner) {
            if part.kind() == "string" {
                if let Ok(text) = part.utf8_text(source) {
                    let pkg = text.trim_matches(|c| c == '"' || c == '\'');
                    if CLIENT_ONLY_PACKAGE_PREFIXES
                        .iter()
                        .any(|prefix| pkg.starts_with(prefix))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn scan_client_apis(node: tree_sitter::Node<'_>, source: &[u8], found: &mut bool) {
    if *found {
        return;
    }
    // Skip import statements entirely — `import { useState }` doesn't prove
    // the file actually uses the hook. Only real call sites / references
    // in the body should count.
    if matches!(node.kind(), "import_statement" | "import_clause") {
        return;
    }
    match node.kind() {
        "identifier" | "property_identifier" | "shorthand_property_identifier" => {
            if let Ok(text) = node.utf8_text(source) {
                if text.starts_with("use") && text.len() > 3 {
                    let next = text.as_bytes()[3];
                    if next.is_ascii_uppercase() {
                        *found = true;
                        return;
                    }
                }
                if text.starts_with("on") && text.len() > 2 {
                    let next = text.as_bytes()[2];
                    if next.is_ascii_uppercase() {
                        *found = true;
                        return;
                    }
                }
                if CLIENT_GLOBALS.contains(&text) {
                    *found = true;
                    return;
                }
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        scan_client_apis(child, source, found);
        if *found {
            return;
        }
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = ctx;
    if !has_use_client_directive(node, source) {
        return;
    }
    if imports_from_client_only_package(node, source) {
        return;
    }
    let mut found = false;
    scan_client_apis(node, source, &mut found);
    if found {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`\"use client\"` directive with no hooks, event handlers, or browser APIs — \
         remove the directive or justify it with client-only behavior."
            .into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_use_client_without_client_api() {
        let src = r#""use client";
export function Title() { return <h1>Hi</h1>; }
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_client_with_hook() {
        let src = r#""use client";
import { useState } from "react";
export function Counter() { const [n, setN] = useState(0); return <div>{n}</div>; }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_client_with_event_handler() {
        let src = r#""use client";
export function B() { return <button onClick={() => {}}>x</button>; }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_client_with_browser_global() {
        let src = r#""use client";
export function H() { const h = window.location.href; return <div>{h}</div>; }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_file_without_use_client() {
        let src = r#"export function T() { return <h1>Hi</h1>; }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_use_client_with_unused_import() {
        // useState is imported but never referenced in the body — the file
        // does not actually need to be a client component.
        let src = r#""use client";
import { useState } from "react";
export function Title() { return <h1>Hi</h1>; }
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression tests for #458 — wrappers around client-only UI primitives
    #[test]
    fn no_fp_for_base_ui_wrapper() {
        // @base-ui/react primitives use focus management and event listeners internally.
        let src = r#""use client";
import * as AlertDialog from "@base-ui/react/alert-dialog";
export const Root = AlertDialog.Root;
export const Trigger = AlertDialog.Trigger;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_base_ui_root_package() {
        let src = r#""use client";
import { Tabs } from "@base-ui/react";
export { Tabs };
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_radix_ui_wrapper() {
        // @radix-ui/* primitives use keyboard nav and pointer events internally.
        let src = r#""use client";
import * as Tooltip from "@radix-ui/react-tooltip";
export const Provider = Tooltip.Provider;
export const Root = Tooltip.Root;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_unrelated_bare_use_client() {
        // A file with no client-only package imports should still be flagged.
        let src = r#""use client";
export function Title() { return <h1>Hi</h1>; }
"#;
        assert_eq!(run(src).len(), 1);
    }
}
