//! prisma-no-findmany-without-take backend — flag `findMany({ ... })` calls
//! whose options object lacks a `take:` or `first:` key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

/// Find every `.findMany(` call and report it if its argument object does
/// not contain a `take:` or `first:` key.
fn find_violations(source: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let needle = ".findMany(";
    let mut from = 0usize;
    while let Some(rel) = source[from..].find(needle) {
        let start = from + rel;
        let after = start + needle.len();
        let mut depth = 1;
        let mut i = after;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        let mut body_end = i.saturating_sub(1);
        while body_end > after && !source.is_char_boundary(body_end) {
            body_end -= 1;
        }
        let body = &source[after..body_end];
        let trimmed = body.trim();
        // Empty `findMany()` is also unbounded.
        let bounded = trimmed.contains("take:")
            || trimmed.contains("take :")
            || trimmed.contains("first:")
            || trimmed.contains("first :");
        if !bounded {
            let (line, col) = byte_to_line_col(source, start + 1);
            out.push((line, col));
        }
        from = i;
    }
    out
}

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_prisma_file(ctx.source) {
            return Vec::new();
        }
        find_violations(ctx.source)
            .into_iter()
            .map(|(line, column)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`findMany()` without `take`/`first` returns unbounded results — add a row limit."
                    .into(),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_findmany_without_take() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ where: { active: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_findmany_no_args() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findmany_with_take() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ take: 50 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_findmany_with_first() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ first: 50 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_files() {
        let src = "const rows = client.user.findMany();";
        assert!(run(src).is_empty());
    }
}
