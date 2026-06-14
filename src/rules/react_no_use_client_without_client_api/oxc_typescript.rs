//! OXC backend for react-no-use-client-without-client-api.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, JSXAttributeName,
};
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

/// Component-factory / higher-order-component call names that build React
/// components which use hooks, `forwardRef`, and context internally. A module
/// whose only client behavior is binding a component to one of these calls
/// (`export const Box = chakra("div")`,
/// `export const Badge = withContext("span")`) is inherently a client module,
/// even though no hook is called directly in the file. Matched only when the
/// name is the callee of a call expression, so importing or re-exporting the
/// factory itself does not count. These are Chakra UI's styled-system factories
/// (`chakra`), context factories (`createRecipeContext`,
/// `createSlotRecipeContext`), and the HOCs they return (`withContext`,
/// `withProvider`, `withRootProvider`).
const CLIENT_COMPONENT_FACTORY_CALLS: &[&str] = &[
    "chakra",
    "createRecipeContext",
    "createSlotRecipeContext",
    "withContext",
    "withProvider",
    "withRootProvider",
];

/// Packages whose re-exports implicitly use client APIs (hooks, event listeners,
/// resize observers, etc.) that are invisible to static analysis. The `next/*`
/// entries are Next.js client components/hooks that already ship `"use client"`,
/// so a barrel re-export propagating the directive is the idiomatic pattern.
const CLIENT_ONLY_PACKAGE_PREFIXES: &[&str] = &[
    "@base-ui/react",
    "@radix-ui/",
    "motion/react",
    "framer-motion",
    "next/link",
    "next/image",
    "next/navigation",
    "next/router",
    "next/headers",
    "next/dynamic",
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
                    let Some(source) = &export.source else {
                        continue;
                    };
                    // A barrel re-exporting from a relative sibling
                    // (`export { Accordion } from './accordion'`) propagates the
                    // sibling's client-ness; the `"use client"` marks the package
                    // entry point for bundlers even though the barrel calls no
                    // hooks itself. The sibling is invisible to single-file
                    // analysis, so the relative re-export is the signal.
                    if is_relative_specifier(source.value.as_str()) {
                        return Vec::new();
                    }
                    for spec in &export.specifiers {
                        if is_hook_name(spec.local.name().as_str()) {
                            return Vec::new();
                        }
                    }
                }
                AstKind::ExportAllDeclaration(export) => {
                    if is_relative_specifier(export.source.value.as_str()) {
                        return Vec::new();
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
                // JSX event-handler props: `onClick`, `onChange`, … . The
                // attribute name is a `JSXIdentifier` inside a `JSXAttribute`,
                // not an `IdentifierName`, so it needs its own arm.
                AstKind::JSXAttribute(attr) => {
                    if let JSXAttributeName::Identifier(ident) = &attr.name
                        && is_event_handler_name(ident.name.as_str())
                    {
                        found_client_api = true;
                        break;
                    }
                }
                // JSX spread props (`<input {...registration} />`) can carry
                // event handlers and refs (e.g. React Hook Form's
                // `UseFormRegisterReturn` spreads `onChange`/`onBlur`/`ref`),
                // none of which are visible as named attributes. Spreading onto
                // a JSX element implies potential client-only behavior.
                AstKind::JSXSpreadAttribute(_) => {
                    found_client_api = true;
                    break;
                }
                // Calls to a recognized component-factory / HOC
                // (`chakra("div")`, `createRecipeContext({...})`,
                // `withContext("span")`). The produced component uses hooks and
                // `forwardRef` internally, so the calling file is a client
                // module even with no direct hook usage.
                AstKind::CallExpression(call) => {
                    if let Expression::Identifier(callee) = &call.callee
                        && CLIENT_COMPONENT_FACTORY_CALLS.contains(&callee.name.as_str())
                    {
                        found_client_api = true;
                        break;
                    }
                }
                AstKind::IdentifierName(id) => {
                    let name = id.name.as_str();
                    // Member-access event handlers: `el.onclick = ...`, etc.
                    if is_event_handler_name(name) {
                        found_client_api = true;
                        break;
                    }
                    // Qualified hook calls (`React.useContext`, `React.useState`, …)
                    // and member-access factories (`React.createContext`,
                    // `React.createSvgIcon`). A static-member property is an
                    // `IdentifierName`, so this is the bare-identifier hook check
                    // applied to the qualified form.
                    if is_hook_name(name) || CLIENT_FACTORY_APIS.contains(&name) {
                        found_client_api = true;
                        break;
                    }
                }
                // Defining a hook is itself client API presence:
                // `function useTheme() {}` / named function expressions.
                AstKind::Function(func) => {
                    if let Some(id) = &func.id
                        && is_hook_name(id.name.as_str())
                    {
                        found_client_api = true;
                        break;
                    }
                }
                // `const useX = () => ...` / `const useX = function () {}`.
                AstKind::VariableDeclarator(decl) => {
                    if matches!(
                        decl.init,
                        Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
                    ) && let BindingPattern::BindingIdentifier(id) = &decl.id
                        && is_hook_name(id.name.as_str())
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

/// True for a relative module specifier (`./accordion`, `../shared`), i.e. a
/// sibling file inside the same package rather than an npm dependency.
fn is_relative_specifier(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

/// True for React-hook-shaped names: `use` followed by an uppercase letter
/// (`useState`, `useSuspenseQuery`, …).
fn is_hook_name(name: &str) -> bool {
    name.starts_with("use") && name.len() > 3 && name.as_bytes()[3].is_ascii_uppercase()
}

/// True for DOM event-handler names: `on` followed by an uppercase letter
/// (`onClick`, `onChange`, `onSubmit`, …). These are browser-only APIs.
fn is_event_handler_name(name: &str) -> bool {
    name.starts_with("on") && name.len() > 2 && name.as_bytes()[2].is_ascii_uppercase()
}

fn is_client_api_name(name: &str) -> bool {
    if is_hook_name(name) {
        return true;
    }
    if is_event_handler_name(name) {
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

    // Regression tests for #2004 — qualified hook calls and hook definitions.
    #[test]
    fn no_fp_for_qualified_react_hook_call_oxc() {
        let src = r#"'use client';
import * as React from 'react';
export default function useTheme<T = DefaultTheme>(): T {
  const theme = React.useContext(ThemeContext);
  React.useDebugValue(theme);
  return theme;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_hook_definition_with_qualified_state_oxc() {
        let src = r#"'use client';
import * as React from 'react';
export function useThing() { return React.useState(0); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_use_client_without_any_client_api_oxc() {
        let src = r#"'use client';
export const x = 1;
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression tests for #2006 — barrel files re-exporting Next.js client-only
    // packages (next/link, next/image, …) legitimately need `"use client"`.
    #[test]
    fn no_fp_for_next_link_reexport_oxc() {
        let src = r#"'use client';
import Link from 'next/link';

export default Link;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_next_image_reexport_oxc() {
        let src = r#"'use client';
import Image from 'next/image';

export default Image;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_client_package_reexport_oxc() {
        let src = r#"'use client';
import { chunk } from 'lodash';

export default chunk;
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression test for #1810 — a `"use client"` component whose only
    // client-side behavior is an explicit JSX event-handler prop (`onClick`).
    #[test]
    fn no_fp_for_jsx_on_click_handler_oxc() {
        let src = r#""use client";

export default function Error({ reset }: { reset: () => void }) {
  return (
    <div className="...">
      <h2>Oh no!</h2>
      <p>There was an issue with our storefront.</p>
      <button className="..." onClick={() => reset()}>
        Try Again
      </button>
    </div>
  );
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_jsx_on_change_handler_oxc() {
        let src = r#""use client";

export function Input() {
  return <input onChange={(e) => console.log(e)} />;
}
"#;
        assert!(run(src).is_empty());
    }

    // Regression test for #1764 — a `"use client"` component whose only
    // client-side behavior is a JSX spread of an event-handler object
    // (React Hook Form's `UseFormRegisterReturn` spreads `onChange`/`onBlur`/`ref`).
    #[test]
    fn no_fp_for_jsx_spread_event_handler_props_oxc() {
        let src = r#"'use client';

import { UseFormRegisterReturn } from 'react-hook-form';

type SelectFieldProps = {
  defaultValue?: string;
  registration: Partial<UseFormRegisterReturn>;
};

export const Select = (props: SelectFieldProps) => {
  const { defaultValue, registration } = props;
  return (
    <select defaultValue={defaultValue} {...registration}>
      <option value="a">A</option>
    </select>
  );
};
"#;
        assert!(run(src).is_empty());
    }

    // Regression tests for #1781 — files that build components via Chakra UI's
    // component-factory / HOC calls, which use hooks and `forwardRef` internally.
    #[test]
    fn no_fp_for_chakra_factory_call_oxc() {
        let src = r#""use client"

import { type HTMLChakraProps, chakra } from "../../styled-system"

export interface BoxProps extends HTMLChakraProps<"div"> {}

export const Box = chakra("div")
Box.displayName = "Box"
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_create_recipe_context_factory_call_oxc() {
        let src = r#""use client"

import {
  type HTMLChakraProps,
  type RecipeProps,
  type UnstyledProp,
  createRecipeContext,
} from "../../styled-system"

export interface BadgeProps extends HTMLChakraProps<"span"> {}

export const { PropsProvider, withContext } = createRecipeContext({ key: "badge" })
export const Badge = withContext<HTMLSpanElement, BadgeProps>("span")
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_factory_call_oxc() {
        let src = r#""use client";
import { chunk } from "lodash";

export const parts = chunk([1, 2, 3], 2);
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression tests for #1795 — a `"use client"` barrel `index.ts` that only
    // re-exports from a relative sibling file (`./accordion`) carrying the
    // actual hooks. The directive marks the package entry point for bundlers.
    #[test]
    fn no_fp_for_relative_named_reexport_barrel_oxc() {
        let src = r#"'use client';
export {
  Accordion,
  AccordionItem,
  AccordionHeader,
  AccordionTrigger,
  AccordionContent,
} from './accordion';
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_relative_export_all_barrel_oxc() {
        let src = r#"'use client';
export * from './accordion';
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_for_parent_relative_reexport_barrel_oxc() {
        let src = r#"'use client';
export { Button } from '../button';
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_package_named_reexport_without_client_api_oxc() {
        let src = r#"'use client';
export { isEqual } from 'lodash';
"#;
        assert_eq!(run(src).len(), 1);
    }
}
