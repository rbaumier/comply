//! better-auth-client-framework-import — flag imports from `better-auth/client` barrel.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(source_node) = node.child_by_field_name("source") else { return };
    let import_path = source_node
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    if import_path != "better-auth/client" {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &source_node,
        super::META.id,
        "Import from a framework-specific path (`better-auth/react`, `/vue`, `/svelte`, `/solid`) instead of `better-auth/client`.".into(),
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_generic_client_import() {
        assert_eq!(
            run("import { createAuthClient } from \"better-auth/client\"").len(),
            1
        );
    }

    #[test]
    fn allows_react_client_import() {
        assert!(run("import { createAuthClient } from \"better-auth/react\"").is_empty());
    }

    #[test]
    fn allows_vue_client_import() {
        assert!(run("import { createAuthClient } from \"better-auth/vue\"").is_empty());
    }
}
