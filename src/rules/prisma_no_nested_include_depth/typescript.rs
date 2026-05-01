//! prisma-no-nested-include-depth backend — count the nesting depth of `include:`
//! keys inside Prisma options objects and flag depths > 3.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_prisma_file(source: &str) -> bool {
    source.contains("@prisma/client")
        || source.contains("PrismaClient")
        || source.contains("prisma.")
}

/// Walk `source` brace-by-brace, tracking how many `include:` keys are
/// "open" on the brace stack at any point. If a new `include:` appears
/// while `MAX_DEPTH` are already open, that's a violation.
fn find_violations(source: &str, max_depth: usize) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    // Stack holds `true` for braces opened immediately after `include:`.
    let mut stack: Vec<bool> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'{' => {
                // Look back for "include:" preceding this `{`.
                let look_back = source[..i].trim_end();
                let is_include = look_back.ends_with("include:")
                    || look_back.ends_with("include :")
                    || look_back.ends_with("\"include\":")
                    || look_back.ends_with("'include':");
                if is_include {
                    let depth = stack.iter().filter(|&&b| b).count() + 1;
                    if depth > max_depth {
                        let (line, col) = byte_to_line_col(source, i);
                        out.push((line, col, depth));
                    }
                }
                stack.push(is_include);
                i += 1;
            }
            b'}' => {
                stack.pop();
                i += 1;
            }
            // Skip string literals so braces inside strings don't confuse us.
            b'"' | b'\'' => {
                let quote = b;
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                i = i.saturating_add(1);
            }
            b'`' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'`' {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                i = i.saturating_add(1);
            }
            // Skip line comments.
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            // Skip block comments.
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = i.saturating_add(2);
            }
            _ => i += 1,
        }
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
        let max_depth = ctx.config.threshold("prisma-no-nested-include-depth", "max_depth", ctx.lang);
        find_violations(ctx.source, max_depth)
            .into_iter()
            .map(|(line, column, depth)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`include:` is nested {depth} levels deep — keep nesting at or below {max_depth} to avoid huge join queries."
                ),
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
    fn flags_four_levels_deep() {
        let src = r#"
import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();
await prisma.user.findMany({
  include: {
    posts: {
      include: {
        comments: {
          include: {
            author: { include: { profile: true } }
          }
        }
      }
    }
  }
});
"#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_three_levels_deep() {
        let src = r#"
import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();
await prisma.user.findMany({
  include: {
    posts: {
      include: {
        comments: { include: { author: true } }
      }
    }
  }
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_include() {
        let src = r#"
import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();
await prisma.user.findMany({ include: { posts: true } });
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_files() {
        let src =
            "const x = { include: { a: { include: { b: { include: { c: { include: 1 } } } } } } };";
        assert!(run(src).is_empty());
    }
}
