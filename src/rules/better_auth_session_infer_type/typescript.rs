//! better-auth-session-infer-type — prefer `typeof auth.$Infer.Session` over manual declarations.

use crate::diagnostic::{Diagnostic, Severity};

fn file_imports_better_auth(source: &[u8]) -> bool {
    let text = std::str::from_utf8(source).unwrap_or("");
    text.contains("from \"better-auth") || text.contains("from 'better-auth")
}

crate::ast_check! { on ["interface_declaration", "type_alias_declaration"] prefilter = ["better-auth"] => |node, source, ctx, diagnostics|
    let Some(name) = node.child_by_field_name("name") else { return };
    if name.utf8_text(source).unwrap_or("") != "Session" {
        return;
    }

    if !file_imports_better_auth(source) {
        return;
    }

    // If the declaration already uses $Infer, skip.
    if node.utf8_text(source).unwrap_or("").contains("$Infer") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Manual `Session` declaration — use `type Session = typeof auth.$Infer.Session` instead.".into(),
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
    fn flags_manual_interface_session() {
        let src = r#"
            import { betterAuth } from "better-auth";
            export interface Session { userId: string }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_manual_type_session() {
        let src = r#"
            import { betterAuth } from "better-auth";
            export type Session = { userId: string };
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_infer_session() {
        let src = r#"
            import { betterAuth } from "better-auth";
            export type Session = typeof auth.$Infer.Session;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_when_no_better_auth_import() {
        let src = "export interface Session { userId: string }";
        assert!(run(src).is_empty());
    }
}
