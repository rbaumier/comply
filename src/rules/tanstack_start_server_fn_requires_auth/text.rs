use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("createServerFn") {
            return vec![];
        }
        if !src.contains(".insert(") && !src.contains(".update(") && !src.contains(".delete(") {
            return vec![];
        }
        if src.contains("getSession(")
            || src.contains("auth()")
            || src.contains("verifySession")
            || src.contains("requireAuth")
            || src.contains("currentUser(")
        {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("createServerFn") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` with mutations must verify authentication before proceeding.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("api.functions.ts"), src))
    }

    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(
            run("const del = createServerFn().handler(async () => { await db.delete(posts) })")
                .len(),
            1
        );
    }

    #[test]
    fn allows_with_get_session() {
        assert!(run(
            "const del = createServerFn().handler(async () => { const s = await getSession(); await db.delete(posts) })"
        )
        .is_empty());
    }

    #[test]
    fn allows_read_only() {
        assert!(
            run("const get = createServerFn().handler(async () => db.select().from(posts))")
                .is_empty()
        );
    }
}
