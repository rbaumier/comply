//! better-auth-no-disable-origin-check — flag `disableOriginCheck: true`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("").trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "disableOriginCheck" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.utf8_text(source).unwrap_or("").trim() != "true" {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`disableOriginCheck: true` removes origin validation — remove this option.".into(),
        Severity::Error,
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
    fn flags_disable_origin() {
        assert_eq!(run("betterAuth({ disableOriginCheck: true })").len(), 1);
    }

    #[test]
    fn allows_trusted_origins() {
        assert!(run("betterAuth({ trustedOrigins: ['https://app.example.com'] })").is_empty());
    }

    #[test]
    fn allows_disable_origin_false() {
        assert!(run("betterAuth({ disableOriginCheck: false })").is_empty());
    }
}
