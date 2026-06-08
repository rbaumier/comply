//! Flags barrel files — modules whose only purpose is to re-export symbols
//! from other modules.
//!
//! A file is considered a barrel when:
//! - it has at least 3 top-level `export ... from '...'` statements, AND
//! - it contains no other code at the top level (comments are ignored).
//!
//! Barrel files degrade tree-shaking, obscure the real import graph, and
//! encourage cyclic dependencies. Consumers should import directly from the
//! source modules instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let root = tree.root_node();
        if root.kind() != "program" {
            return Vec::new();
        }

        let barrel_threshold = ctx.config.threshold("avoid-barrel-files", "min_reexports", ctx.lang);

        let mut reexport_count = 0usize;
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "comment" | "hash_bang_line" => continue,
                "export_statement" => {
                    if child.child_by_field_name("source").is_some() {
                        reexport_count += 1;
                    } else {
                        // An export that isn't a re-export ({@code export function foo}
                        // or {@code export const x = ...}) means the file has its own
                        // code — not a pure barrel.
                        return Vec::new();
                    }
                }
                _ => return Vec::new(),
            }
        }

        if reexport_count < barrel_threshold {
            return Vec::new();
        }

        let pos = root.start_position();
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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
