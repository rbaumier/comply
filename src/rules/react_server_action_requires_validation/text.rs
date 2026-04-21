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
        if src.contains(".parse(") || src.contains(".safeParse(") || src.contains(".input(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if !t.starts_with("export async function") {
                continue;
            }
            if let Some(open) = t.find('(') {
                let after = &t[open + 1..];
                if let Some(close) = after.find(')')
                    && close > 0
                {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Server Action with parameters must validate input with `.parse()` or `.safeParse()`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
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
    fn flags_params_no_parse() {
        assert_eq!(
            run("'use server'\nexport async function del(id: string) { await db.delete(x) }")
                .len(),
            1
        );
    }

    #[test]
    fn allows_with_parse() {
        assert!(run(
            "'use server'\nexport async function del(input: unknown) { schema.parse(input); }"
        )
        .is_empty());
    }

    #[test]
    fn allows_no_params() {
        assert!(
            run("'use server'\nexport async function list() { return db.select() }").is_empty()
        );
    }

    #[test]
    fn allows_non_server_file() {
        assert!(
            run("export async function del(id: string) { await db.delete(x) }").is_empty()
        );
    }
}
