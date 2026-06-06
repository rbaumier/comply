//! zod-validate-env-at-startup oxc backend — flag `process.env.X` in Zod files
//! that never parse `process.env` through a schema.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn file_uses_zod(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from \"zod\"")
        || crate::oxc_helpers::source_contains(source, "from 'zod'")
        || crate::oxc_helpers::source_contains(source, "require(\"zod\")")
        || crate::oxc_helpers::source_contains(source, "require('zod')")
}

fn file_validates_env(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, ".parse(process.env)") || crate::oxc_helpers::source_contains(source, ".safeParse(process.env)")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process.env"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !file_uses_zod(ctx.source) || file_validates_env(ctx.source) {
            return;
        }

        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        use oxc_ast::ast::Expression;

        // Must be `process.env.X` — object is `process.env` (another StaticMemberExpression).
        let Expression::StaticMemberExpression(inner) = &member.object else {
            return;
        };
        let Expression::Identifier(obj) = &inner.object else {
            return;
        };
        if obj.name != "process" {
            return;
        }
        if inner.property.name != "env" {
            return;
        }

        let var_name = member.property.name.as_str();
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`process.env.{}` is read without a Zod `parse(process.env)` \
                 guard — validate env vars once at startup and read them from \
                 the typed result.",
                var_name,
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_unvalidated_env_in_zod_file() {
        let src = r#"
            import { z } from "zod";
            const port = process.env.PORT;
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_validated_env() {
        let src = r#"
            import { z } from "zod";
            const schema = z.object({ PORT: z.string() });
            const env = schema.parse(process.env);
            const port = env.PORT;
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_safe_parse_validation() {
        let src = r#"
            import { z } from "zod";
            const schema = z.object({ PORT: z.string() });
            const result = schema.safeParse(process.env);
            if (!result.success) throw new Error("bad env");
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_files_without_zod() {
        let src = "const port = process.env.PORT;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_multiple_accesses() {
        let src = r#"
            import { z } from "zod";
            const port = process.env.PORT;
            const host = process.env.HOST;
        "#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn ignores_process_exit() {
        let src = r#"
            import { z } from "zod";
            process.exit(1);
        "#;
        assert!(run_on(src).is_empty());
    }
}
