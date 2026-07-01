//! OxcCheck backend — flag `.appendChild()` calls.

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
        Some(&["appendChild"])
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
        if member.property.name.as_str() != "appendChild" {
            return;
        }

        // The rule is type-dependent: `.append()` only exists on the DOM
        // `ParentNode` interface. When the project defines its own class method
        // named `appendChild`, the receiver is a project tree node (HTML/XML
        // AST, vdom, scene graph, …) that has no `.append()`, so suggesting it
        // would break the code. The call site and the class definition routinely
        // live in different files, so this is a project-wide signal.
        if ctx
            .project
            .import_index()
            .project_defines_dom_tree_method("appendChild")
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
            message: "Prefer `Node#append()` over `Node#appendChild()`.".into(),
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

    /// Single-file run with an empty project (no user-defined `appendChild`).
    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    /// Build a real multi-file project, then run the rule on `target_rel` with
    /// the cross-file `ImportIndex` populated — the only way to exercise the
    /// project-wide `appendChild`-method signal.
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
    fn flags_dom_append_child() {
        let d = run("parent.appendChild(child);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("append"));
    }

    #[test]
    fn flags_document_body_append_child() {
        assert_eq!(run("document.body.appendChild(child);").len(), 1);
    }

    #[test]
    fn allows_append() {
        assert!(run("parent.append(child);").is_empty());
    }

    // Regression for #7012: nativescript-vue's `NSVNode` class defines its own
    // `appendChild` and has no `append()`; the call (`this.appendChild(el)`)
    // lives in the subclass `NSVElement`, in a different file from the base
    // class. The class definition is therefore cross-file from the call site, so
    // the signal must be project-wide.
    #[test]
    fn no_fp_when_project_defines_append_child_cross_file() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/node.ts",
                "export abstract class NSVNode {\n  appendChild(el: NSVNode): NSVNode {\n    this.childNodes.push(el);\n    return el;\n  }\n}",
            ),
            (
                "src/element.ts",
                "import { NSVNode } from './node';\nexport class NSVElement extends NSVNode {\n  insertBefore(el: NSVNode, anchor?: NSVNode | null) {\n    if (!anchor) {\n      return this.appendChild(el);\n    }\n  }\n}",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/element.ts");
        assert!(
            diags.is_empty(),
            "must not flag .appendChild() when the project defines its own appendChild method: {diags:?}"
        );
    }

    // True-positive guard: a project with NO user-defined `appendChild` is real
    // DOM code, so `document.body.appendChild(child)` must still flag.
    #[test]
    fn flags_dom_append_child_when_project_has_no_user_method() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/helper.ts",
                "export function noop() {}",
            ),
            (
                "src/dom.ts",
                "export function attach(el: HTMLElement, child: HTMLElement) {\n  el.appendChild(child);\n}",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "src/dom.ts");
        assert_eq!(
            diags.len(),
            1,
            "real DOM appendChild must still flag when no user-defined appendChild exists: {diags:?}"
        );
    }
}
