//! imports-first OxcCheck backend.
//!
//! Walks all top-level statements via `run_on_semantic`. Import declarations
//! after a non-import statement are flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Declaration, Expression, Statement};
use std::sync::Arc;

pub struct Check;

/// A directive prologue is an expression statement whose expression is a
/// string literal (e.g. `"use strict";`, `"use client";`).
fn is_directive(stmt: &Statement) -> bool {
    matches!(stmt, Statement::ExpressionStatement(expr)
        if matches!(&expr.expression, Expression::StringLiteral(_)))
}

/// Test-framework configuration calls that are conventionally placed before
/// imports:
/// - `jest.setTimeout(N)` — sets the default test timeout for the file
/// - `vi.setConfig({ testTimeout: N })` — Vitest equivalent
/// - `jest.mock(...)` / `jest.unmock(...)` / `vi.mock(...)` / `vi.unmock(...)` —
///   module mocks that the test runner hoists above the imports they mock, so
///   placing them before imports is required for the mock to take effect
/// - `jasmine.DEFAULT_TIMEOUT_INTERVAL = N` — Jasmine equivalent (assignment)
///
/// These are zero-import-side-effect statements and must not flip `saw_non_import`.
fn is_test_framework_config(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };

    // `jest.setTimeout(N)` / `vi.setConfig(...)` / `jest.mock(...)` etc.
    if let Expression::CallExpression(call) = &expr_stmt.expression
        && let Expression::StaticMemberExpression(member) = &call.callee
        && let Expression::Identifier(obj) = &member.object
    {
        return matches!(
            (obj.name.as_str(), member.property.name.as_str()),
            ("jest", "setTimeout")
                | ("vi", "setConfig")
                | ("jest" | "vi", "mock" | "unmock")
        );
    }

    // `jasmine.DEFAULT_TIMEOUT_INTERVAL = N`
    if let Expression::AssignmentExpression(assign) = &expr_stmt.expression
        && let AssignmentTarget::StaticMemberExpression(member) = &assign.left
        && let Expression::Identifier(obj) = &member.object
    {
        return obj.name.as_str() == "jasmine"
            && member.property.name.as_str() == "DEFAULT_TIMEOUT_INTERVAL";
    }

    false
}

/// A `process.env.X = value` (or `process.env["X"] = value`) assignment. In a
/// test setup file these set feature-flag environment variables that imported
/// modules read during their own initialization, so they must be placed before
/// the imports — the same zero-import-side-effect class as the test-framework
/// configuration calls. Only exempted in test files (see the call site), since
/// mutating `process.env` mid-module is a genuine smell in ordinary source.
fn is_process_env_assignment(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
        return false;
    };

    // The assignment target is `process.env.<X>` (static) or
    // `process.env[<expr>]` (computed); in both the member's object is the
    // `process.env` member expression.
    let target_object = match &assign.left {
        AssignmentTarget::StaticMemberExpression(member) => &member.object,
        AssignmentTarget::ComputedMemberExpression(member) => &member.object,
        _ => return false,
    };

    matches!(
        target_object,
        Expression::StaticMemberExpression(env)
            if matches!(&env.object, Expression::Identifier(obj) if obj.name.as_str() == "process")
                && env.property.name.as_str() == "env"
    )
}

/// TypeScript type-namespace-only declarations: `type X = ...`,
/// `interface X {}`, and `declare`-ambient module/namespace blocks. They are
/// fully erased by the compiler, carry no runtime presence and no import side
/// effects, so they must not flip `saw_non_import` when placed between imports
/// (e.g. a `type Props = {...}` declared next to a component). `enum` is
/// excluded because it emits runtime code.
fn is_type_only_declaration(stmt: &Statement) -> bool {
    match stmt {
        Statement::TSTypeAliasDeclaration(_) | Statement::TSInterfaceDeclaration(_) => true,
        Statement::TSModuleDeclaration(decl) => decl.declare,
        // `export type X = ...`, `export interface X {}`, and `export type { Foo }`.
        Statement::ExportNamedDeclaration(export) => {
            if export.export_kind.is_type() {
                return true;
            }
            matches!(
                &export.declaration,
                Some(Declaration::TSTypeAliasDeclaration(_) | Declaration::TSInterfaceDeclaration(_))
            )
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let body = &semantic.nodes().program().body;
        let mut saw_non_import = false;

        for stmt in body {
            match stmt {
                // ES `import ... from "..."` and TS import-equals declarations
                // (`import X = Namespace.Type;`, `import x = require("x");`) are
                // all import statements subject to the ordering rule.
                Statement::ImportDeclaration(_)
                | Statement::TSImportEqualsDeclaration(_) => {
                    if saw_non_import {
                        let span = match stmt {
                            Statement::ImportDeclaration(d) => d.span,
                            Statement::TSImportEqualsDeclaration(d) => d.span,
                            _ => unreachable!(),
                        };
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Import statement after non-import code \u{2014} move to the top of the file.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                // `export { x } from "./y"` re-exports — conventionally live in the
                // import block. Don't flip the flag on them.
                Statement::ExportNamedDeclaration(decl) if decl.source.is_some() => {}
                Statement::ExportAllDeclaration(_) => {}
                // Directives like "use strict" don't count as real code.
                _ if is_directive(stmt) => {}
                // Empty statements (lone semicolons) are harmless.
                Statement::EmptyStatement(_) => {}
                // Test-framework configuration calls (`jest.setTimeout`,
                // `vi.setConfig`, `jasmine.DEFAULT_TIMEOUT_INTERVAL = N`) placed
                // before imports are a widespread convention with no import side
                // effects — they must not flip `saw_non_import`.
                _ if is_test_framework_config(stmt) => {}
                // `process.env.X = value` in a test file sets a feature-flag
                // env var that imported modules read at initialization time, so
                // it must precede the imports — exempt only inside test files,
                // where it has no import side effects of its own.
                _ if ctx.file.path_segments.in_test_dir
                    && is_process_env_assignment(stmt) => {}
                // TypeScript type-only declarations (`type`/`interface`,
                // `declare` modules, `export type`) are erased at compile time
                // and have no import side effects — they must not break the
                // imports block.
                _ if is_type_only_declaration(stmt) => {}
                _ => {
                    saw_non_import = true;
                }
            }
        }

        diagnostics
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        let path = std::path::Path::new(path);
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            source,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    // Regression test for https://github.com/rbaumier/comply/issues/2047
    // A `type` alias declared between two imports is type-namespace-only and
    // erased at compile time — it must not flag the following import.
    #[test]
    fn allows_type_alias_between_imports() {
        let src = r#"import { Link, routes } from '@redwoodjs/router'
import { Metadata } from '@redwoodjs/web'

type BlogPostPageProps = {
  id: number
}

import BlogPostCell from 'src/components/BlogPostCell'
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_interface_between_imports() {
        let src = r#"import { a } from 'a'

interface Props {
  id: number
}

import { b } from 'b'
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_export_type_and_interface_between_imports() {
        let src = r#"import { a } from 'a'

export type Props = { id: number }
export interface State { count: number }
export type { Helper } from './helper'

import { b } from 'b'
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_declare_module_between_imports() {
        let src = r#"import { a } from 'a'

declare module 'virtual:config' {
  export const value: string
}

import { b } from 'b'
"#;
        assert!(run_on(src).is_empty());
    }

    // True positive: a runtime statement between imports must still flag the
    // following import. The type-only exemption must not weaken this.
    #[test]
    fn still_flags_runtime_statement_between_imports() {
        let src = r#"import { a } from 'a'

const x = compute()

import { b } from 'b'
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // A runtime `enum` emits code, so it is not exempt and must still flag.
    #[test]
    fn still_flags_enum_between_imports() {
        let src = r#"import { a } from 'a'

enum Color { Red, Green }

import { b } from 'b'
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression test for https://github.com/rbaumier/comply/issues/1899
    // Jest hoists `jest.mock()` calls above the imports it mocks, so placing
    // them before imports is required, not disorganized — they must not flag
    // the following imports.
    #[test]
    fn allows_jest_mock_before_imports() {
        let src = r#"jest.mock('react', () => {
  const actual = jest.requireActual('react');
  return { ...actual, cache: (fn) => fn };
});
jest.mock('../utils/isRsc', () => ({ IS_RSC: true }));

import React from 'react';
import ReactDOMServer from 'react-dom/server';
"#;
        assert!(run_on(src).is_empty());
    }

    // Regression test for https://github.com/rbaumier/comply/issues/1143
    // `import X = Namespace.Type;` is a TypeScript namespace import alias: a
    // declaration with no runtime effect. Placed between ES imports it must not
    // flip the imports block and flag the following imports.
    #[test]
    fn allows_namespace_import_alias_between_imports() {
        let src = r#"import { LRUCache } from "lru-cache";
import LRUCacheOptions = LRUCache.Options;
import { logger } from "./logger.js";
import { DateType } from "./logicalTypes/dateType.js";
"#;
        assert!(run_on(src).is_empty());
    }

    // An `import x = require("x")` (CommonJS import-equals) is the legacy TS
    // equivalent of an ES import and likewise belongs in the import block.
    #[test]
    fn allows_import_equals_require_between_imports() {
        let src = r#"import { a } from "a";
import fs = require("fs");
import { b } from "b";
"#;
        assert!(run_on(src).is_empty());
    }

    // A genuine runtime statement before an import-equals declaration must still
    // flag it — import-equals is an import statement, so it is subject to the
    // same ordering rule as ES imports.
    #[test]
    fn still_flags_import_equals_after_runtime_code() {
        let src = r#"import { a } from "a";

const x = compute();

import LRUCacheOptions = LRUCache.Options;
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // `jest.unmock()` / `vi.mock()` / `vi.unmock()` follow the same hoisting
    // convention and must not flip the imports block either.
    #[test]
    fn allows_unmock_and_vi_mock_before_imports() {
        let src = r#"jest.unmock('../legacy');
vi.mock('./db');
vi.unmock('./cache');

import { a } from 'a';
import { b } from 'b';
"#;
        assert!(run_on(src).is_empty());
    }

    // Regression test for https://github.com/rbaumier/comply/issues/1745
    // A `process.env.X = value` assignment in a test setup file must precede the
    // imports so imported modules read the env var at initialization time — it
    // must not flag the following imports.
    #[test]
    fn allows_process_env_assignment_before_imports_in_test_file() {
        let src = r#"process.env.NEW_SLOT_SYNTAX = 'true'

import './helpers/shim-done'
import './helpers/to-have-warned'
import './helpers/classlist'

import { waitForUpdate } from './helpers/wait-for-update'
import { triggerEvent } from './helpers/trigger-event'
import { createTextVNode } from './helpers/vdom'
"#;
        assert!(run_on_path(src, "test/vitest.setup.ts").is_empty());
    }

    // The computed form `process.env['X'] = value` is the same env-setup pattern.
    #[test]
    fn allows_computed_process_env_assignment_before_imports_in_test_file() {
        let src = r#"process.env['NODE_ENV'] = 'test'

import { setup } from './setup'
"#;
        assert!(run_on_path(src, "test/setup.spec.ts").is_empty());
    }

    // The exemption is test-only: a `process.env.X = value` before imports in
    // ordinary source is a genuine non-import statement and must still flag.
    #[test]
    fn still_flags_process_env_assignment_in_non_test_file() {
        let src = r#"process.env.FORCE_COLOR = '1'

import { run } from './run'
"#;
        assert_eq!(run_on_path(src, "src/main.ts").len(), 1);
    }
}
