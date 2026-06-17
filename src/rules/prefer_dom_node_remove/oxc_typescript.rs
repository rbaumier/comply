//! OxcCheck backend — flag `.removeChild()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["removeChild"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "removeChild" {
            return;
        }

        // The rule is type-dependent: `.remove()` only exists on the DOM
        // `ChildNode` interface. When the project defines its own class method
        // named `removeChild`, the receiver is a project tree node (HTML/XML
        // AST, vdom, scene graph, …) that has no `.remove()`, so suggesting it
        // would break the code. The call site and the class definition routinely
        // live in different files, so this is a project-wide signal.
        if ctx
            .project
            .import_index()
            .project_defines_remove_child_method()
        {
            return;
        }

        // Report at the property location (matching tree-sitter behaviour).
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::CheckCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Single-file run with an empty project (no user-defined `removeChild`).
    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    /// Build a real multi-file project, then run the rule on `target_rel` with
    /// the cross-file `ImportIndex` populated — the only way to exercise the
    /// project-wide `removeChild`-method signal.
    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let canon = fs::canonicalize(&target_path).unwrap();

        let source_type = crate::oxc_helpers::source_type_for_path(&canon);
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, &source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, &source, &project);

        // Per-node dispatch, mirroring `test_helpers::run_oxc_check` (this rule
        // implements `run`, not `run_on_semantic`).
        let kinds = Check.interested_kinds();
        let mut diags = Vec::new();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diags);
            }
        }
        (dir, diags)
    }

    #[test]
    fn flags_dom_remove_child() {
        let d = run("parent.removeChild(child);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("remove"));
    }

    #[test]
    fn flags_parent_node_remove_child() {
        assert_eq!(run("el.parentNode.removeChild(el);").len(), 1);
    }

    #[test]
    fn allows_remove() {
        assert!(run("child.remove();").is_empty());
    }

    // Regression for #3916: prettier's HTML AST `Node` class (in `ast.js`)
    // defines its own `removeChild` and has no `remove()`; the calls live in a
    // different file (`print-preprocess.js`). The class definition is therefore
    // cross-file from the call site, so the signal must be project-wide.
    #[test]
    fn no_fp_when_project_defines_remove_child_cross_file() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/ast.js",
                "class Node {\n  removeChild(child) {\n    const c = this.$children;\n    c.splice(c.indexOf(child), 1);\n  }\n}\nexport { Node };",
            ),
            (
                "src/print-preprocess.js",
                "export function preprocess(ast) {\n  ast.walk((node) => {\n    node.removeChild(node.children[0]);\n  });\n}",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/print-preprocess.js");
        assert!(
            diags.is_empty(),
            "must not flag .removeChild() when the project defines its own removeChild method: {diags:?}"
        );
    }

    // True-positive guard: a project with NO user-defined `removeChild` is real
    // DOM code, so `parent.removeChild(child)` must still flag.
    #[test]
    fn flags_dom_remove_child_when_project_has_no_user_method() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/helper.ts",
                "export function noop() {}",
            ),
            (
                "src/dom.ts",
                "export function detach(el: HTMLElement) {\n  el.parentNode.removeChild(el);\n}",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/dom.ts");
        assert_eq!(
            diags.len(),
            1,
            "real DOM removeChild must still flag when no user-defined removeChild exists: {diags:?}"
        );
    }
}
