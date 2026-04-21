//! tanstack-start-no-client-import-in-server-fn backend — scoped to
//! `*.functions.ts(x)` files, flags imports of client-only React hooks
//! or `react-dom` packages that cannot run on the server.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

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

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let file_name = ctx
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !(file_name.ends_with(".functions.ts") || file_name.ends_with(".functions.tsx")) {
            return vec![];
        }

        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if !trimmed.starts_with("import") {
                continue;
            }

            // Flag any import from react-dom / react-dom/client.
            if line.contains("from \"react-dom")
                || line.contains("from 'react-dom")
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "`react-dom` is client-only and cannot be imported from a server-function file."
                            .into(),
                    severity: Severity::Error,
                    span: None,
                });
                continue;
            }

            // Flag imports of client-only React hooks (by name appearing in the import line).
            for hook in CLIENT_HOOKS {
                if let Some(col) = find_identifier(line, hook) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{hook}` is a client-only React hook and cannot be imported from a server-function file."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
        }
        diags
    }
}

/// Find `needle` in `haystack` bounded by non-identifier characters.
fn find_identifier(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let nlen = needle.len();
    let mut start = 0;
    while let Some(rel) = haystack[start..].find(needle) {
        let abs = start + rel;
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after_idx = abs + nlen;
        let after_ok = after_idx >= bytes.len() || !is_ident_byte(bytes[after_idx]);
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + nlen;
    }
    None
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
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
        let diags = run(
            "src/users/foo.functions.ts",
            "import { z } from 'zod'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_functions_file() {
        let diags = run(
            "src/users/regular.ts",
            "import { useState } from 'react'",
        );
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
