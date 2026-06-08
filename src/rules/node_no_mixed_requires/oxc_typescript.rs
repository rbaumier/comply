//! node-no-mixed-requires oxc backend — don't mix require() with other declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

fn is_require_init(init: &Expression) -> bool {
    if let Expression::CallExpression(call) = init
        && let Expression::Identifier(id) = &call.callee {
            return id.name.as_str() == "require";
        }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for stmt in &semantic.nodes().program().body {
            let decl = match stmt {
                Statement::VariableDeclaration(d) => d,
                _ => continue,
            };
            if decl.declarations.len() < 2 {
                continue;
            }
            let mut has_require = false;
            let mut has_other = false;
            for declarator in &decl.declarations {
                if let Some(init) = &declarator.init {
                    if is_require_init(init) {
                        has_require = true;
                    } else {
                        has_other = true;
                    }
                } else {
                    has_other = true;
                }
            }
            if has_require && has_other {
                let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Do not mix `require` and other declarations.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_mixed_declarations() {
        let d = run_on("var fs = require('fs'), foo = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("mix"));
    }


    #[test]
    fn allows_only_requires() {
        assert!(run_on("var fs = require('fs'), path = require('path');").is_empty());
    }


    #[test]
    fn allows_only_non_requires() {
        assert!(run_on("var a = 1, b = 2;").is_empty());
    }
}
