//! node-no-process-env backend — flag `process.env` usage.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true when `node` is nested inside a `.parse()` or `.safeParse()`
/// call — the Zod centralized-env-reader pattern where raw env vars are
/// collected once and validated through a schema.
fn is_inside_schema_parse_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "call_expression" {
            if let Some(func) = ancestor.child_by_field_name("function") {
                if func.kind() == "member_expression" {
                    if let Some(prop) = func.child_by_field_name("property") {
                        let prop_text = prop.utf8_text(source).unwrap_or("");
                        if matches!(prop_text, "parse" | "safeParse") {
                            return true;
                        }
                    }
                }
            }
        }
        current = ancestor.parent();
    }
    false
}

crate::ast_check! { on ["member_expression"] prefilter = ["process"] => |node, source, ctx, diagnostics|
    let Some(obj) = node.child_by_field_name("object") else { return };
    let Some(prop) = node.child_by_field_name("property") else { return };

    if obj.kind() != "identifier" || obj.utf8_text(source).unwrap_or("") != "process" {
        return;
    }
    if prop.utf8_text(source).unwrap_or("") != "env" {
        return;
    }

    if is_inside_schema_parse_call(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-process-env".into(),
        message: "Unexpected use of `process.env`. Centralize environment access in a config module.".into(),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_process_env() {
        let d = run_on("const port = process.env.PORT;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("process.env"));
    }

    #[test]
    fn flags_process_env_bracket() {
        let d = run_on("const x = process.env['NODE_ENV'];");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_other_process_members() {
        assert!(run_on("process.exit(1);").is_empty());
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
    fn allows_process_env_inside_schema_safe_parse() {
        let src = r#"
function readEnv() {
  return EnvSchema.safeParse({
    port: process.env['PORT'],
  });
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_process_env_outside_parse() {
        let src = r#"
function readSentryEnv() {
  return SentryEnvSchema.parse({ dsn: process.env['DSN'] });
}
const scattered = process.env['SCATTERED'];
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("process.env"));
    }
}
