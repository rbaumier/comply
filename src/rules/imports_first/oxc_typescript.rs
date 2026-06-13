//! imports-first OxcCheck backend.
//!
//! Walks all top-level statements via `run_on_semantic`. Import declarations
//! after a non-import statement are flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Expression, Statement};
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
/// - `jasmine.DEFAULT_TIMEOUT_INTERVAL = N` — Jasmine equivalent (assignment)
///
/// These are zero-import-side-effect statements and must not flip `saw_non_import`.
fn is_test_framework_config(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };

    // `jest.setTimeout(N)` / `vi.setConfig(...)`
    if let Expression::CallExpression(call) = &expr_stmt.expression
        && let Expression::StaticMemberExpression(member) = &call.callee
        && let Expression::Identifier(obj) = &member.object
    {
        return matches!(
            (obj.name.as_str(), member.property.name.as_str()),
            ("jest", "setTimeout") | ("vi", "setConfig")
        );
    }

    // `jasmine.DEFAULT_TIMEOUT_INTERVAL = N`
    if let Expression::AssignmentExpression(assign) = &expr_stmt.expression
        && let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left
        && let Expression::Identifier(obj) = &member.object
    {
        return obj.name.as_str() == "jasmine"
            && member.property.name.as_str() == "DEFAULT_TIMEOUT_INTERVAL";
    }

    false
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
                Statement::ImportDeclaration(_) => {
                    if saw_non_import {
                        let span = match stmt {
                            Statement::ImportDeclaration(d) => d.span,
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
}
