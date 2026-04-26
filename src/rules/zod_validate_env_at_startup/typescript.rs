//! zod-validate-env-at-startup backend — flag `process.env.X` in Zod files
//! that never parse `process.env` through a schema.
//!
//! Scope: fires only in files that import `zod`. Otherwise `node-no-process-env`
//! already covers the broader "don't touch process.env directly" case.
//!
//! Heuristic: the file opts into Zod, so the author is expected to validate
//! env vars through a schema. We consider the file compliant when it contains
//! `.parse(process.env)` or `.safeParse(process.env)` somewhere — the typical
//! shape of `const env = envSchema.parse(process.env)`. If that sentinel is
//! absent, every `process.env.FOO` read is flagged.

use crate::diagnostic::{Diagnostic, Severity};

/// Return `true` if the file imports Zod.
fn file_uses_zod(source: &str) -> bool {
    source.contains("from \"zod\"")
        || source.contains("from 'zod'")
        || source.contains("require(\"zod\")")
        || source.contains("require('zod')")
}

/// Return `true` if the file already validates `process.env` via a Zod
/// schema call (`.parse(process.env)` or `.safeParse(process.env)`).
fn file_validates_env(source: &str) -> bool {
    source.contains(".parse(process.env)")
        || source.contains(".safeParse(process.env)")
}

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    // Short-circuit: if the file doesn't use Zod or already validates env,
    // skip every node. Cheap substring checks on the full source.
    if !file_uses_zod(ctx.source) || file_validates_env(ctx.source) {
        return;
    }

    // Match `process.env.X` — object is itself the `process.env` member
    // expression, property is any identifier.
    let Some(obj) = node.child_by_field_name("object") else { return };
    let Some(prop) = node.child_by_field_name("property") else { return };
    if obj.kind() != "member_expression" {
        return;
    }
    let Some(inner_obj) = obj.child_by_field_name("object") else { return };
    let Some(inner_prop) = obj.child_by_field_name("property") else { return };

    if inner_obj.utf8_text(source).unwrap_or("") != "process" {
        return;
    }
    if inner_prop.utf8_text(source).unwrap_or("") != "env" {
        return;
    }

    let pos = node.start_position();
    let var_name = prop.utf8_text(source).unwrap_or("?");
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-validate-env-at-startup".into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
        // Non-Zod files are handled by `node-no-process-env`, not here.
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
        // `process.exit(1)` is not `process.env.X` — don't flag.
        let src = r#"
            import { z } from "zod";
            process.exit(1);
        "#;
        assert!(run_on(src).is_empty());
    }
}
