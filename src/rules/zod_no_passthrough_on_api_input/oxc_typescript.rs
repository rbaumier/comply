use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ROUTE_HINTS: &[&str] = &["route", "api", "handler", "controller", "endpoint"];

fn looks_like_api_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    ROUTE_HINTS.iter().any(|h| s.contains(h))
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

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".passthrough("])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !looks_like_api_path(ctx.path) {
            return Vec::new();
        }
        if !ctx.source_contains(".passthrough(") {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = ctx.source[from..].find(".passthrough(") {
            let abs = from + rel;
            let (line, column) = byte_to_line_col(ctx.source, abs);
            out.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`.passthrough()` on an API input schema lets clients smuggle unknown keys — use `.strict()` or omit.".into(),
                severity: Severity::Warning,
                span: None,
            });
            from = abs + 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_passthrough_in_route_file() {
        let src = "const Body = z.object({ name: z.string() }).passthrough();";
        assert_eq!(run_at(src, "src/routes/users.ts").len(), 1);
    }

    #[test]
    fn flags_passthrough_in_api_file() {
        let src = "const Body = z.object({}).passthrough();";
        assert_eq!(run_at(src, "src/api/things.ts").len(), 1);
    }

    #[test]
    fn allows_passthrough_in_unrelated_file() {
        let src = "const Body = z.object({}).passthrough();";
        assert!(run_at(src, "src/lib/util.ts").is_empty());
    }

    #[test]
    fn allows_strict_in_route_file() {
        let src = "const Body = z.object({}).strict();";
        assert!(run_at(src, "src/routes/users.ts").is_empty());
    }
}
