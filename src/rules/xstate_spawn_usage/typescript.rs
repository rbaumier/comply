//! xstate-spawn-usage — flag `spawn(...)` calls that are not nested inside
//! an `assign(...)` call. In XState v5, `spawn` must only be invoked from
//! within an `assign` action so the spawned actor is tracked by the machine.

use crate::diagnostic::{Diagnostic, Severity};

/// Return true if `node` is a `call_expression` whose callee is the plain
/// identifier `assign`.
fn is_assign_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" {
        return false;
    }
    callee.utf8_text(source).unwrap_or("") == "assign"
}

crate::ast_check! { on ["call_expression"] prefilter = ["spawn"] => |node, source, ctx, diagnostics|
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else { return };
    if !pkg.has_dep_or_engine("xstate") { return; }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "spawn" {
        return;
    }

    // Walk ancestors; if any is an `assign(...)` call, we're fine.
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if is_assign_call(ancestor, source) {
            return;
        }
        current = ancestor.parent();
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`spawn()` must be called inside an `assign()` action.".into(),
        Severity::Warning,
    ));
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
    use std::fs;
    use tempfile::TempDir;

    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;

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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, canon.to_str().unwrap(), &project, &crate::rules::file_ctx::FileCtx::default())
    }

    fn run_no_xstate(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_spawn_outside_assign() {
        let diags = run_xstate("const actor = spawn(childMachine);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_spawn_inside_unrelated_call() {
        let diags = run_xstate("doStuff(spawn(childMachine));");
        assert_eq!(diags.len(), 1);
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
