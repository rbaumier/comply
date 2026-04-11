use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn imports_hono(source: &str) -> bool {
    source.contains("from 'hono'") || source.contains("from \"hono\"")
}

fn has_csrf_protection(source: &str) -> bool {
    source.contains("hono/csrf")
}

fn has_mutation_route(line: &str) -> bool {
    line.contains(".post(")
        || line.contains(".put(")
        || line.contains(".delete(")
        || line.contains(".patch(")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !imports_hono(ctx.source) || has_csrf_protection(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_mutation_route(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-csrf-missing".into(),
                    message: "Mutation route without CSRF protection — add `app.use(csrf())`.".into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
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
    fn flags_post_without_csrf() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.post('/api', handler);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_mutation_routes() {
        let src = "import { Hono } from 'hono';\napp.post('/a', h);\napp.put('/b', h);\napp.delete('/c', h);";
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn allows_with_csrf_import() {
        let src = "import { Hono } from 'hono';\nimport { csrf } from 'hono/csrf';\napp.use(csrf());\napp.post('/api', handler);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.post('/api', handler);";
        assert!(run(src).is_empty());
    }
}
