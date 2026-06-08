//! OxcCheck backend for avoid-barrel-files.
//!
//! Uses `run_on_semantic` to scan the entire program for re-exports.
//! A file is a barrel when it has >= threshold re-export statements
//! and no other top-level code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

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
        let program = semantic.nodes().program();
        let barrel_threshold = ctx.config.threshold("avoid-barrel-files", "min_reexports", ctx.lang);

        let mut reexport_count = 0usize;

        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(decl) => {
                    if decl.source.is_some() {
                        reexport_count += 1;
                    } else {
                        return Vec::new();
                    }
                }
                Statement::ExportAllDeclaration(_) => {
                    reexport_count += 1;
                }
                Statement::ExportDefaultDeclaration(_) => {
                    return Vec::new();
                }
                _ => {
                    return Vec::new();
                }
            }
        }

        if reexport_count < barrel_threshold {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Barrel file — {reexport_count} re-exports and no other code. Import directly from source modules."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_pure_barrel_file() {
        let src = "\
export { a } from './a';
export { b } from './b';
export { c } from './c';
";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_barrel_with_star_reexports() {
        let src = "\
export * from './a';
export * from './b';
export { c, d } from './c';
";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_two_reexports_below_threshold() {
        let src = "\
export { a } from './a';
export { b } from './b';
";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_mixed_file_with_local_code() {
        let src = "\
export { a } from './a';
export { b } from './b';
export { c } from './c';
export function helper() { return 1; }
";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_normal_module() {
        let src = "\
import { x } from './x';
export function doStuff() { return x + 1; }
export const y = 2;
";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_top_level_comments() {
        let src = "\
// Public API surface.
export { a } from './a';
export { b } from './b';
export { c } from './c';
";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn does_not_flag_when_imports_present() {
        // Top-level imports mean the file pulls in code beyond re-exports.
        let src = "\
import './side-effect';
export { a } from './a';
export { b } from './b';
export { c } from './c';
";
        assert!(run(src).is_empty());
    }
}
