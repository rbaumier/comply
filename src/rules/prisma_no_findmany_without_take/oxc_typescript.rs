use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn find_violations(source: &str) -> Vec<usize> {
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
        let bounded = trimmed.contains("take:")
            || trimmed.contains("take :")
            || trimmed.contains("first:")
            || trimmed.contains("first :");
        if !bounded {
            out.push(start + 1); // point to the `.`
        }
        from = i;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findMany"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_prisma_file(ctx.source) {
            return Vec::new();
        }
        find_violations(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`findMany()` without `take`/`first` returns unbounded results — \
                              add a row limit."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_findmany_without_take() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst rows = await prisma.user.findMany({ where: { active: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_findmany_no_args() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst rows = await prisma.user.findMany();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findmany_with_take() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst rows = await prisma.user.findMany({ take: 50 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_files() {
        let src = "const rows = client.user.findMany();";
        assert!(run(src).is_empty());
    }
}
