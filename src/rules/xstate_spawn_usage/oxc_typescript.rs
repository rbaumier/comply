//! xstate-spawn-usage OXC backend.
//!
//! Flag `spawn(...)` calls not nested inside an `assign(...)` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["spawn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Must be bare `spawn(...)`.
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "spawn" {
            return;
        }

        // Must have xstate dependency.
        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else { return };
        if !pkg.has_dep_or_engine("xstate") {
            return;
        }

        // Walk ancestors; if any is an `assign(...)` call, we're fine.
        let nodes = semantic.nodes();
        let mut cur_id = nodes.parent_id(node.id());
        loop {
            if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
                break;
            }
            if let AstKind::CallExpression(ancestor_call) = nodes.kind(cur_id)
                && let Expression::Identifier(id) = &ancestor_call.callee
                    && id.name.as_str() == "assign" {
                        return;
                    }
            cur_id = nodes.parent_id(cur_id);
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`spawn()` must be called inside an `assign()` action.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;



    fn run_xstate(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"xstate":"^5"}}"#,
        )
        .unwrap();
        let file_path = dir.path().join("src/machine.ts");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            &project)
    }


    fn run_no_xstate(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_spawn_inside_assign() {
        assert!(
            run_xstate("const action = assign({ ref: () => spawn(childMachine) });").is_empty()
        );
    }


    #[test]
    fn allows_spawn_inside_assign_with_context_arg() {
        assert!(
            run_xstate("const action = assign((ctx) => ({ ref: spawn(childMachine) }));")
                .is_empty()
        );
    }


    #[test]
    fn allows_no_spawn_call() {
        assert!(run_no_xstate("const x = foo(childMachine);").is_empty());
    }


    #[test]
    fn skips_non_xstate_project() {
        assert!(run_no_xstate("const actor = spawn(childMachine);").is_empty());
    }
}
