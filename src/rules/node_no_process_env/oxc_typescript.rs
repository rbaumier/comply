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
    CONFIG_STEMS.iter().any(|s| stem.eq_ignore_ascii_case(s))
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
}
