//! OxcCheck backend — flag `process.env` usage.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const CONFIG_STEMS: &[&str] = &["config", "env", "environment"];

fn is_config_file(ctx: &CheckCtx) -> bool {
    let stem = ctx
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if CONFIG_STEMS.iter().any(|s| stem.eq_ignore_ascii_case(s)) {
        return true;
    }
    // playwright.config.ts, vite.config.ts, drizzle.config.ts, vitest.config.ts, etc.
    stem.to_ascii_lowercase().ends_with(".config")
}

/// Returns true when `node` is nested inside a `.parse()` or `.safeParse()`
/// call — the Zod centralized-env-reader pattern.
fn is_inside_schema_parse_call<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    node: &oxc_semantic::AstNode<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            if matches!(member.property.name.as_str(), "parse" | "safeParse") {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if is_config_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::StaticMemberExpression(member) = node.kind() else { continue };
            if member.property.name.as_str() != "env" {
                continue;
            }
            let oxc_ast::ast::Expression::Identifier(obj) = &member.object else { continue };
            if obj.name.as_str() != "process" {
                continue;
            }

            if is_inside_schema_parse_call(semantic, node) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, member.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message:
                    "Unexpected use of `process.env`. Centralize environment access in a config module."
                        .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_process_env() {
        let d = run_on("const port = process.env.PORT;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("process.env"));
    }

    #[test]
    fn allows_config_file() {
        assert!(run_on_path("const env = process.env;", "src/config.ts").is_empty());
    }

    #[test]
    fn allows_env_file() {
        assert!(run_on_path("const env = process.env;", "src/env.ts").is_empty());
    }

    #[test]
    fn allows_environment_file() {
        assert!(run_on_path("export default process.env;", "environment.js").is_empty());
    }

    #[test]
    fn still_flags_in_regular_file() {
        let d = run_on_path("const env = process.env;", "src/server.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_other_process_members() {
        assert!(run_on("process.exit(1);").is_empty());
    }

    // Regression tests for #443: *.config.ts files must be exempt
    #[test]
    fn allows_playwright_config() {
        let src = "export default defineConfig({ use: { baseURL: process.env.BASE_URL ?? 'http://localhost:3000' } });";
        assert!(run_on_path(src, "playwright.config.ts").is_empty());
    }

    #[test]
    fn allows_vite_config() {
        let src = "export default defineConfig({ define: { __API_URL__: JSON.stringify(process.env.API_URL) } });";
        assert!(run_on_path(src, "vite.config.ts").is_empty());
    }

    #[test]
    fn allows_drizzle_config() {
        let src = "export default { connectionString: process.env.DATABASE_URL };";
        assert!(run_on_path(src, "drizzle.config.ts").is_empty());
    }

    #[test]
    fn allows_vitest_config() {
        let src = "export default defineConfig({ test: { env: { BASE: process.env.BASE } } });";
        assert!(run_on_path(src, "vitest.config.ts").is_empty());
    }

    #[test]
    fn still_flags_in_non_config_ts() {
        let d = run_on_path("const x = process.env.FOO;", "app.config-helper.ts");
        assert_eq!(d.len(), 1);
    }

    // Regression: #501 — centralized env-reader pattern using Zod .parse()
    #[test]
    fn allows_process_env_inside_schema_parse() {
        let src = r#"
const SentryEnvSchema = z.object({
  sentryDsn: z.string().optional(),
  nodeEnv: z.enum(['development', 'production']).default('development'),
});
function readSentryEnv() {
  return SentryEnvSchema.parse({
    sentryDsn: process.env['API_OBSERVABILITY_SENTRY_DSN'],
    nodeEnv: process.env['API_SERVER_NODE_ENV'],
  });
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_process_env_inside_safe_parse() {
        let src = r#"
function readEnv() {
  return EnvSchema.safeParse({ port: process.env.PORT });
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_process_env_outside_parse() {
        let src = r#"
function readSentryEnv() {
  return SentryEnvSchema.parse({ dsn: process.env.DSN });
}
const scattered = process.env.SCATTERED;
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
