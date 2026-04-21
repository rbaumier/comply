use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TABLE_DEFS: &[&str] = &["pgTable(", "mysqlTable(", "sqliteTable(", "table("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let has_drizzle =
            ctx.source.contains("drizzle-orm") || ctx.source.contains("drizzle-zod");
        let has_zod = ctx.source.contains("from 'zod'") || ctx.source.contains("from \"zod\"");
        let has_table = TABLE_DEFS.iter().any(|t| ctx.source.contains(t));
        let uses_generator = ctx.source.contains("createInsertSchema")
            || ctx.source.contains("createSelectSchema");

        if !has_drizzle || !has_zod || !has_table || uses_generator {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.trim().contains("z.object(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "drizzle-zod-prefer-generated-schema".into(),
                    message: "Manual `z.object({})` in a Drizzle schema file likely duplicates column definitions — use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_manual_zod_in_drizzle_file() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core'
import { z } from 'zod'
export const users = pgTable('users', { name: text('name') })
export const insertUserSchema = z.object({ name: z.string() })
"#;
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_generated_schema() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core'
import { createInsertSchema } from 'drizzle-zod'
export const users = pgTable('users', { name: text('name') })
export const insertUserSchema = createInsertSchema(users)
"#;
        assert!(run(src).is_empty());
    }
    #[test]
    fn ignores_non_drizzle_zod_files() {
        let src = r#"
import { z } from 'zod'
export const schema = z.object({ name: z.string() })
"#;
        assert!(run(src).is_empty());
    }
}
