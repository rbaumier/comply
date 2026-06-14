//! imports-first OxcCheck backend.
//!
//! Walks all top-level statements via `run_on_semantic`. Import declarations
//! after a non-import statement are flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, AssignmentTarget, Declaration, Expression, Statement};
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

/// The global-object identifiers whose properties hold runtime configuration
/// read by modules at import time.
const GLOBAL_OBJECTS: [&str; 4] = ["global", "globalThis", "window", "self"];

/// Peels `(expr)`, `expr as T`, `expr satisfies T`, and `expr!` wrappers to the
/// underlying expression, so a target like `(global as any)` resolves to the
/// `global` identifier.
fn unwrap_casts<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    loop {
        current = match current {
            Expression::ParenthesizedExpression(p) => &p.expression,
            Expression::TSAsExpression(a) => &a.expression,
            Expression::TSSatisfiesExpression(s) => &s.expression,
            Expression::TSNonNullExpression(n) => &n.expression,
            _ => return current,
        };
    }
}

/// An assignment to a property of a global object, e.g.
/// `(global as any).isNode = true`, `globalThis.__DEV__ = false`,
/// `window.foo = bar`. The assigned property holds runtime configuration that
/// modules imported afterwards read during their own initialization, so the
/// assignment must precede those imports — the same zero-import-side-effect
/// ordering class as the test-framework configuration calls. It is name-agnostic
/// (no filename match) and stays narrow: the target's base object must be one of
/// the recognized global identifiers, so `const x = compute()` or `obj.x = 1` on
/// an ordinary object is unaffected and still flips the imports block.
fn is_global_config_assignment(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
        return false;
    };

    let target_object = match &assign.left {
        AssignmentTarget::StaticMemberExpression(member) => &member.object,
        AssignmentTarget::ComputedMemberExpression(member) => &member.object,
        _ => return false,
    };

    matches!(
        unwrap_casts(target_object),
        Expression::Identifier(obj) if GLOBAL_OBJECTS.contains(&obj.name.as_str())
    )
}

/// A `require("pkg")` call expression: the callee is the bare `require`
/// identifier and the first argument is a string-literal module specifier.
fn is_require_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    callee.name.as_str() == "require"
        && matches!(call.arguments.first(), Some(Argument::StringLiteral(_)))
}

/// A `const`/`let`/`var x = require("pkg")` declaration (every declarator
/// initialized with a `require(...)` call). In a file mixing CommonJS and ES
/// module syntax this is a module-loading statement equivalent to an `import`,
/// not imperative code, so it must not flip `saw_non_import` when it precedes an
/// ES `import`. A declaration with any non-`require` initializer (e.g.
/// `const x = compute()`) is genuine logic and stays flagged.
fn is_require_declaration(stmt: &Statement) -> bool {
    let Statement::VariableDeclaration(decl) = stmt else {
        return false;
    };
    !decl.declarations.is_empty()
        && decl
            .declarations
            .iter()
            .all(|declarator| declarator.init.as_ref().is_some_and(is_require_call))
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
                // `(global as any).x = value` / `globalThis.x = value` etc. set
                // runtime configuration on a global object that modules imported
                // afterwards read at initialization time, so the assignment must
                // precede those imports — it must not flip `saw_non_import`.
                _ if is_global_config_assignment(stmt) => {}
                // TypeScript type-only declarations (`type`/`interface`,
                // `declare` modules, `export type`) are erased at compile time
                // and have no import side effects — they must not break the
                // imports block.
                _ if is_type_only_declaration(stmt) => {}
                // `const x = require("pkg")` (CommonJS) is a module-loading
                // declaration, not imperative code. In a mixed CJS/ESM file it
                // legitimately precedes ES imports, so it must not break the
                // imports block.
                _ if is_require_declaration(stmt) => {}
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

    // Regression test for https://github.com/rbaumier/comply/issues/1746
    // A `const x = require('pkg')` (CommonJS) before ES imports in a mixed
    // CJS/ESM file is a module-loading declaration, not imperative code — it must
    // not flag the following imports.
    #[test]
    fn allows_require_declaration_before_imports() {
        let src = r#"const postcss = require('postcss')
import { ProcessOptions, LazyResult } from 'postcss'
import trimPlugin from './stylePlugins/trim'
import scopedPlugin from './stylePlugins/scoped'
"#;
        assert!(run_on(src).is_empty());
    }

    // Multiple `require()` declarations before imports are likewise module loads.
    #[test]
    fn allows_multiple_require_declarations_before_imports() {
        let src = r#"const path = require('path')
const serialize = require('serialize-javascript')
import { isJS, isCSS } from '../util'
import TemplateStream from './template-stream'
"#;
        assert!(run_on(src).is_empty());
    }

    // A genuine non-`require` initializer before an import is real logic and must
    // still flag — the require exemption must not weaken the rule.
    #[test]
    fn still_flags_non_require_declaration_before_imports() {
        let src = r#"const x = compute()
import { a } from 'a'
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // A declaration mixing a `require()` and a non-`require` initializer is not a
    // pure module load, so it must still flag the following import.
    #[test]
    fn still_flags_mixed_require_and_call_declaration_before_imports() {
        let src = r#"const fs = require('fs'), y = compute()
import { a } from 'a'
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression test for https://github.com/rbaumier/comply/issues/2300
    // An Angular test-init file interleaves side-effect imports, global
    // configuration assignments, and framework imports in a required order: the
    // globals must be set before the imports that read them. The
    // `(global as any).x = ...` assignments are runtime config, not imperative
    // code, so the imports after them must not flag.
    #[test]
    fn allows_global_config_assignment_between_imports() {
        let src = r#"import 'reflect-metadata';
import 'zone.js';

(global as any).isNode = true;
(global as any).isBrowser = false;

import '@angular/compiler';
import {NgModule, provideZonelessChangeDetection} from '@angular/core';
import {TestBed} from '@angular/core/testing';
"#;
        assert!(run_on(src).is_empty());
    }

    // The other global-object identifiers (`globalThis`, `window`, `self`) and
    // the plain (uncast) member form are the same runtime-config pattern.
    #[test]
    fn allows_uncast_global_object_assignments_between_imports() {
        let src = r#"import 'polyfill';
globalThis.__DEV__ = false;
window.config = { debug: true };
self.workerReady = true;
import { app } from './app';
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative-space guard: a genuine non-import statement (a real computation
    // and a side-effecting call, not a global-object assignment) before an import
    // is a true ordering violation and must still flag.
    #[test]
    fn still_flags_runtime_code_before_import_with_global_exemption() {
        let src = r#"import { compute, doWork } from './lib'

const x = compute()
doWork(x)

import { b } from 'b'
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // An assignment to a property of an ordinary (non-global) object is genuine
    // imperative code and must still flag the following import — the exemption
    // is narrow to the recognized global identifiers.
    #[test]
    fn still_flags_ordinary_object_property_assignment_before_import() {
        let src = r#"import { config } from './config'

config.ready = true

import { b } from 'b'
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
