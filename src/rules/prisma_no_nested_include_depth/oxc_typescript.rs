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

fn find_violations(source: &str, max_depth: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut stack: Vec<bool> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'{' => {
                let look_back = source[..i].trim_end();
                let is_include = look_back.ends_with("include:")
                    || look_back.ends_with("include :")
                    || look_back.ends_with("\"include\":")
                    || look_back.ends_with("'include':");
                if is_include {
                    let depth = stack.iter().filter(|&&b| b).count() + 1;
                    if depth > max_depth {
                        out.push((i, depth));
                    }
                }
                stack.push(is_include);
                i += 1;
            }
            b'}' => {
                stack.pop();
                i += 1;
            }
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
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
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

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["include:"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_prisma_file(ctx.source) {
            return Vec::new();
        }
        let max_depth =
            ctx.config
                .threshold("prisma-no-nested-include-depth", "max_depth", ctx.lang);
        find_violations(ctx.source, max_depth)
            .into_iter()
            .map(|(offset, depth)| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`include:` is nested {depth} levels deep — keep nesting at or below \
                         {max_depth} to avoid huge join queries."
                    ),
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
