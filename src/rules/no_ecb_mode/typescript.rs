//! no-ecb-mode backend — flag ECB cipher mode in string literals.

use crate::diagnostic::{Diagnostic, Severity};

fn contains_ecb(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-ecb") {
        return true;
    }
    if lower.contains(".ecb") {
        return true;
    }
    // mode: 'ecb' — both tokens would be in the same string
    if lower == "ecb" {
        return true;
    }
    false
}

crate::ast_check! { on ["string_fragment"] => |node, source, ctx, diagnostics|
    // Only check string_fragment to avoid double-counting (string parent also matches).
    let Ok(text) = node.utf8_text(source) else { return };
    if !contains_ecb(text) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ecb-mode".into(),
        message: "ECB cipher mode is insecure — use CBC, CTR, or GCM instead.".into(),
        severity: Severity::Error,
        span: None,
    });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_aes_ecb() {
        assert_eq!(run_on("createCipheriv('aes-128-ecb', key, iv)").len(), 1);
    }

    #[test]
    fn flags_aes_256_ecb() {
        assert_eq!(run_on("algorithm: 'aes-256-ecb'").len(), 1);
    }

    #[test]
    fn allows_cbc_mode() {
        assert!(run_on("createCipheriv('aes-128-cbc', key, iv)").is_empty());
    }

    #[test]
    fn allows_gcm_mode() {
        assert!(run_on("createCipheriv('aes-256-gcm', key, iv)").is_empty());
    }
}
