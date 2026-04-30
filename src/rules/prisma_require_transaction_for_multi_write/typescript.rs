//! prisma-require-transaction-for-multi-write — file-level rule.
//!
//! Counts Prisma write calls (`create`, `update`, `delete`, `upsert`,
//! including the `Many` variants) in the file. If there are 2 or more
//! and the file does not call `$transaction`, fire a single diagnostic
//! at the first write.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
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
    source.contains("@prisma/client")
        || source.contains("PrismaClient")
        || source.contains("prisma.")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_prisma_file(ctx.source) {
            return Vec::new();
        }
        if ctx.source.contains("$transaction(") {
            return Vec::new();
        }

        // Collect (byte_offset, method) for each write call, scoped to lines
        // that look like prisma writes (preceded by `prisma`, `tx`, or end of model identifier).
        let bytes = ctx.source.as_bytes();
        let mut hits: Vec<usize> = Vec::new();
        for method in WRITE_METHODS {
            let mut from = 0usize;
            while let Some(rel) = ctx.source[from..].find(method) {
                let pos = from + rel;
                // Heuristic: only count if the call sits on a `prisma.<model>.<method>(` chain.
                // Look back to find `prisma.` or `tx.` on the same line / just before.
                if looks_like_prisma_call(ctx.source, pos) {
                    hits.push(pos);
                }
                from = pos + method.len();
            }
            let _ = bytes; // touched to silence unused if needed
        }
        hits.sort_unstable();
        hits.dedup();
        if hits.len() < 2 {
            return Vec::new();
        }
        let first = hits[0];
        let (line, column) = byte_to_line_col(ctx.source, first);
        let count = hits.len();
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{count} Prisma write operations in this file without `$transaction(...)` — wrap them so partial failures don't leave inconsistent state."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

/// Walk back from the byte before `.create(` etc. and check whether the
/// chain originates from a `prisma` / `tx` / `db` style identifier. Stops
/// at the first non-`[A-Za-z0-9_$.]` byte.
fn looks_like_prisma_call(source: &str, dot_pos: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = dot_pos;
    // Walk back over the identifier-chain.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
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
