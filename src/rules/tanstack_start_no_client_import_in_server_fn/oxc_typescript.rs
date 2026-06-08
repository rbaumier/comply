//! tanstack-start-no-client-import-in-server-fn oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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

fn is_server_fn_file(ctx: &CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name.ends_with(".functions.ts")
        || file_name.ends_with(".functions.tsx")
        || file_name.ends_with(".server.ts")
        || file_name.ends_with(".server.tsx")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["createServerFn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        if !is_server_fn_file(ctx) {
            return;
        }

        let module_path = import.source.value.as_str();

        if module_path == "react-dom" || module_path.starts_with("react-dom/") {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, import.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`react-dom` is client-only and cannot be imported from a server-function file.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Check named imports for client-only hooks.
        if let Some(specifiers) = &import.specifiers {
            for spec in specifiers {
                if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                    let name = named.imported.name().as_str();
                    if let Some(hook) = CLIENT_HOOKS.iter().find(|h| **h == name) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, named.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{hook}` is a client-only React hook and cannot be imported from a server-function file."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, path)
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

    #[test]
    fn flags_use_state_in_server_file() {
        let diags = run(
            "src/users/foo.server.ts",
            "import { useState } from 'react'",
        );
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_react_dom_in_server_file() {
        let diags = run(
            "src/users/bar.server.tsx",
            "import ReactDOM from 'react-dom'",
        );
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_safe_import_in_server_file() {
        let diags = run("src/users/foo.server.ts", "import { z } from 'zod'");
        assert!(diags.is_empty());
    }
}
