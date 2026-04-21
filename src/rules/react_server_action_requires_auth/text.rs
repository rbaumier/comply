use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.lines().take(5).any(|l| {
            let t = l.trim();
            t == "'use server'" || t == r#""use server""#
        }) {
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
            let t = line.trim();
            if t.starts_with("export async function") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Server Action with mutations must verify authentication before proceeding.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("actions.ts"), src))
    }

    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(
            run("'use server'\nexport async function create(t: string) { await db.insert(posts).values({ t }) }").len(),
            1
        );
    }

    #[test]
    fn allows_with_get_session() {
        assert!(run("'use server'\nexport async function create(t: string) { const s = await getSession(); await db.insert(posts).values({ t }) }").is_empty());
    }

    #[test]
    fn allows_read_only() {
        assert!(
            run("'use server'\nexport async function list() { return db.select().from(posts) }")
                .is_empty()
        );
    }

    #[test]
    fn allows_non_server_file() {
        assert!(run(
            "export async function create(t: string) { await db.insert(posts).values({ t }) }"
        )
        .is_empty());
    }
}
