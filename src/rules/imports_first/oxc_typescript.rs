//! imports-first OxcCheck backend.
//!
//! Walks all top-level statements via `run_on_semantic`. Import declarations
//! after a non-import statement are flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

/// A directive prologue is an expression statement whose expression is a
/// string literal (e.g. `"use strict";`, `"use client";`).
fn is_directive(stmt: &Statement) -> bool {
    matches!(stmt, Statement::ExpressionStatement(expr)
        if matches!(&expr.expression, oxc_ast::ast::Expression::StringLiteral(_)))
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
                _ => {
                    saw_non_import = true;
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;



    fn run_on(s: &str) -> Vec<Diagnostic> {
        run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_import_after_code() {
        let src = r#"const x = 1;
import { a } from 'a';
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }


    #[test]
    fn allows_imports_at_top() {
        let src = r#"import { a } from 'a';
import { b } from 'b';
const x = 1;
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_directive_before_imports() {
        let src = r#"'use strict';
import { a } from 'a';
const x = 1;
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_multiline_import_block_at_top() {
        let src = r#"
import {
  A,
  B,
  C,
} from "x";
import Y from "y";
const z = 1;
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_import_after_non_import() {
        let src = r#"
import A from "x";
const z = 1;
import B from "y";
"#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn ignores_dynamic_import_expression() {
        let src = r#"
const z = 1;
const mod = await import("./y");
const w = 2;
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_blank_lines_between_imports() {
        let src = r#"
import A from "x";

import B from "y";

import C from "z";
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_comments_between_imports() {
        let src = r#"
import A from "x";
// comment
import B from "y";
"#;
        assert!(run_on(src).is_empty());
    }
}
