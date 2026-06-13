//! OXC backend for react-no-use-client-without-client-api.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use std::collections::HashSet;
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

/// Factory calls that produce client-only React values, so a file calling one
/// legitimately needs `"use client"` even without hooks or event handlers:
/// `createContext` (context must be created on the client) and `createSvgIcon`
/// (MUI's component factory wrapping JSX into a client icon component).
const CLIENT_FACTORY_APIS: &[&str] = &["createContext", "createSvgIcon"];

/// Packages whose re-exports implicitly use client APIs (hooks, event listeners,
/// resize observers, etc.) that are invisible to static analysis.
const CLIENT_ONLY_PACKAGE_PREFIXES: &[&str] = &[
    "@base-ui/react",
    "@radix-ui/",
    "motion/react",
    "framer-motion",
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

        // Suppress if the file re-exports from a known client-only package —
        // their primitives use hooks/observers/event listeners internally.
        for node in semantic.nodes().iter() {
            if let AstKind::ImportDeclaration(decl) = node.kind() {
                let pkg = decl.source.value.as_str();
                if CLIENT_ONLY_PACKAGE_PREFIXES
                    .iter()
                    .any(|prefix| pkg.starts_with(prefix))
                {
                    return Vec::new();
                }
            }
        }

        // Collect local binding names whose *imported* (original) name is a hook,
        // so a re-export through an alias still counts. Handles
        // `import { useX as y } from '...'` paired with `export { y }` /
        // `export const z = y`, where the reference is to the alias `y` and the
        // `use*` shape is invisible on the local name. Also treat a direct
        // `export { useX as y } from '...'` as client usage when the *source*
        // name is a hook.
        let mut aliased_hook_locals: HashSet<&str> = HashSet::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ImportDeclaration(decl) => {
                    let Some(specifiers) = &decl.specifiers else {
                        continue;
                    };
                    for spec in specifiers {
                        if let ImportDeclarationSpecifier::ImportSpecifier(named) = spec
                            && is_hook_name(named.imported.name().as_str())
                        {
                            aliased_hook_locals.insert(named.local.name.as_str());
                        }
                    }
                }
                AstKind::ExportNamedDeclaration(export) => {
                    if export.source.is_none() {
                        continue;
                    }
                    for spec in &export.specifiers {
                        if is_hook_name(spec.local.name().as_str()) {
                            return Vec::new();
                        }
                    }
                }
                _ => {}
            }
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
                    if is_client_api_name(name) || aliased_hook_locals.contains(name) {
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
                    // Member-access factories: `React.createContext`, `React.createSvgIcon`.
                    if CLIENT_FACTORY_APIS.contains(&name) {
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

/// True for React-hook-shaped names: `use` followed by an uppercase letter
/// (`useState`, `useSuspenseQuery`, …).
fn is_hook_name(name: &str) -> bool {
    name.starts_with("use") && name.len() > 3 && name.as_bytes()[3].is_ascii_uppercase()
}

fn is_client_api_name(name: &str) -> bool {
    if is_hook_name(name) {
        return true;
    }
    if name.starts_with("on") && name.len() > 2 && name.as_bytes()[2].is_ascii_uppercase() {
        return true;
    }
    if CLIENT_FACTORY_APIS.contains(&name) {
        return true;
    }
    CLIENT_GLOBALS.contains(&name)
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // Regression tests for #458
    #[test]
    fn no_fp_for_base_ui_wrapper_oxc() {
        let src = r#""use client";
import * as AlertDialog from "@base-ui/react/alert-dialog";
export const Root = AlertDialog.Root;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_radix_ui_wrapper_oxc() {
        let src = r#""use client";
import * as Tooltip from "@radix-ui/react-tooltip";
export const Provider = Tooltip.Provider;
"#;
        assert!(run(src).is_empty());
    }

    // Regression tests for #2040 — motion/react (framer-motion v2) animation
    // components are browser-only and legitimately need `"use client"`.
    #[test]
    fn no_fp_for_motion_react_wrapper_oxc() {
        let src = r#"'use client'

import { motion } from 'motion/react'
import type { ReactNode } from 'react'

export const FadeIn = ({ children }: { children: ReactNode }) => (
  <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}>
    {children}
  </motion.div>
)
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_framer_motion_wrapper_oxc() {
        let src = r#""use client";
import { AnimatePresence } from "framer-motion";
export const Wrapper = AnimatePresence;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_bare_use_client_oxc() {
        let src = r#""use client";
export function Title() { return <div>Hi</div>; }
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression tests for #1428 — factory files that legitimately need `"use client"`
    #[test]
    fn no_fp_for_react_create_context_oxc() {
        let src = r#"'use client';
import * as React from 'react';

const ButtonGroupContext = React.createContext({});

export default ButtonGroupContext;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_bare_create_context_oxc() {
        let src = r#"'use client';
import { createContext } from 'react';

const Ctx = createContext(null);

export default Ctx;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_create_svg_icon_factory_oxc() {
        let src = r#"'use client';
import createSvgIcon from '../../utils/createSvgIcon';

export default createSvgIcon(
  <path d="M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z" />,
  'NavigateNext',
);
"#;
        assert!(run(src).is_empty());
    }

    // Regression tests for #2039 — hook re-exports through aliased import names.
    #[test]
    fn no_fp_for_aliased_hook_reexport_const_oxc() {
        let src = r#"'use client'
import {
  useSuspenseQuery as original_useSuspenseQuery,
} from '@tanstack/react-query'

export const useSuspenseQuery = original_useSuspenseQuery
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_aliased_hook_reexport_named_oxc() {
        let src = r#"'use client'
import { useFoo as renamedUseFoo } from './foo'

export { renamedUseFoo }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_direct_aliased_hook_reexport_oxc() {
        let src = r#"'use client'
export { useFoo as useThing } from './foo'
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_hook_aliased_reexport_oxc() {
        let src = r#"'use client'
import { helper as renamedHelper } from './helper'

export const helper = renamedHelper
"#;
        assert_eq!(run(src).len(), 1);
    }
}
