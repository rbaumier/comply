//! tanstack-start-no-client-import-in-server-fn backend — only fires on
//! `*.functions.ts(x)` files. Walks `import_statement` nodes and flags
//! either a `react-dom` import or an import that pulls a client-only
//! React hook (`useState`, `useEffect`, …).

use crate::diagnostic::{Diagnostic, Severity};

const CLIENT_HOOKS: &[&str] = &[
    "useState",
    "useEffect",
    "useLayoutEffect",
    "useRef",
    "useContext",
    "useReducer",
    "useSyncExternalStore",
    "useImperativeHandle",
];

fn is_functions_file(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name.ends_with(".functions.ts") || file_name.ends_with(".functions.tsx")
}

/// Strip a single layer of surrounding quotes from a string literal's
/// raw source text. Tree-sitter's `string` node text includes the quote
/// characters.
fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'' || bytes[0] == b'`')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if !is_functions_file(ctx) { return; }

    // Resolve the imported module path.
    let Some(source_node) = node.child_by_field_name("source") else { return };
    let Ok(raw) = source_node.utf8_text(source) else { return };
    let module_path = strip_quotes(raw);

    if module_path == "react-dom" || module_path.starts_with("react-dom/") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`react-dom` is client-only and cannot be imported from a server-function file.".into(),
            Severity::Error,
        ));
        return;
    }

    // For other modules, walk named imports and flag client-only hooks.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "import_clause" { continue; }
        let mut ic_cursor = child.walk();
        for ic_child in child.children(&mut ic_cursor) {
            if ic_child.kind() != "named_imports" { continue; }
            let mut ni_cursor = ic_child.walk();
            for spec in ic_child.children(&mut ni_cursor) {
                if spec.kind() != "import_specifier" { continue; }
                // The imported name lives in field `name`.
                let Some(name_node) = spec.child_by_field_name("name") else { continue };
                let Ok(name) = name_node.utf8_text(source) else { continue };
                if let Some(hook) = CLIENT_HOOKS.iter().find(|h| **h == name) {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &name_node,
                        super::META.id,
                        format!(
                            "`{hook}` is a client-only React hook and cannot be imported from a server-function file."
                        ),
                        Severity::Error,
                    ));
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, path)
    }

    #[test]
    fn flags_use_state_in_functions_file() {
        let diags = run(
            "src/users/foo.functions.ts",
            "import { useState } from 'react'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_react_dom_import() {
        let diags = run(
            "src/users/bar.functions.ts",
            "import ReactDOM from 'react-dom'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_safe_import() {
        let diags = run("src/users/foo.functions.ts", "import { z } from 'zod'");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_functions_file() {
        let diags = run("src/users/regular.ts", "import { useState } from 'react'");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_multiple_hooks() {
        let diags = run(
            "src/users/foo.functions.tsx",
            "import { useState, useEffect } from 'react'",
        );
        assert_eq!(diags.len(), 1);
    }
}
