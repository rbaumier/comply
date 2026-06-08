use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const WRITE_METHODS: &[&str] = &[
    ".create(",
    ".createMany(",
    ".update(",
    ".updateMany(",
    ".delete(",
    ".deleteMany(",
    ".upsert(",
];

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn looks_like_prisma_call(source: &str, dot_pos: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = dot_pos;
    while i > 0 {
        let prev = bytes[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' || prev == b'.' {
            i -= 1;
        } else {
            break;
        }
    }
    let chain = &source[i..dot_pos];
    chain.starts_with("prisma")
        || chain.starts_with("tx.")
        || chain.starts_with("db.")
        || chain.contains(".prisma.")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["prisma."])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_prisma_file(ctx.source) {
            return Vec::new();
        }
        if ctx.source_contains("$transaction(") {
            return Vec::new();
        }

        let mut hits: Vec<usize> = Vec::new();
        for method in WRITE_METHODS {
            let mut from = 0usize;
            while let Some(rel) = ctx.source[from..].find(method) {
                let pos = from + rel;
                if looks_like_prisma_call(ctx.source, pos) {
                    hits.push(pos);
                }
                from = pos + method.len();
            }
        }
        hits.sort_unstable();
        hits.dedup();
        if hits.len() < 2 {
            return Vec::new();
        }
        let first = hits[0];
        let (line, column) = byte_offset_to_line_col(ctx.source, first);
        let count = hits.len();
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{count} Prisma write operations in this file without `$transaction(...)` — wrap \
                 them so partial failures don't leave inconsistent state."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_two_writes_without_transaction() {
        let src = r#"
import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();
async function f() {
  await prisma.user.create({ data: { name: 'a' } });
  await prisma.post.update({ where: { id: 1 }, data: { title: 't' } });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_two_writes_in_transaction() {
        let src = r#"
import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();
async function f() {
  await prisma.$transaction([
    prisma.user.create({ data: { name: 'a' } }),
    prisma.post.update({ where: { id: 1 }, data: { title: 't' } }),
  ]);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_write() {
        let src = r#"
import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();
async function f() {
  await prisma.user.create({ data: { name: 'a' } });
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_files() {
        let src = "client.user.create({}); client.user.update({});";
        assert!(run(src).is_empty());
    }
}
