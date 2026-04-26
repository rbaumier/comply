//! drizzle-zod-prefer-generated-schema — flag manual `z.object({})`
//! calls in a Drizzle schema file (file imports drizzle-orm, imports
//! zod, defines a `pgTable`/`mysqlTable`/`sqliteTable`/`table`, and
//! does NOT use `createInsertSchema`/`createSelectSchema`).
//!
//! AST detection: walk `call_expression` nodes. We prefilter the file
//! by inspecting the program's `import_statement` children and any
//! `*Table(` calls. If the prefilter passes, every `z.object(...)` call
//! gets flagged.

use crate::diagnostic::{Diagnostic, Severity};

const TABLE_FNS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable", "table"];

#[derive(Default, Clone, Copy)]
struct FileFlags {
    has_drizzle_import: bool,
    has_zod_import: bool,
    has_table_call: bool,
    uses_generator: bool,
}

fn collect_file_flags(root: tree_sitter::Node, source: &[u8]) -> FileFlags {
    let mut flags = FileFlags::default();
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "import_statement" => {
                if let Some(src) = n.child_by_field_name("source") {
                    let txt = src.utf8_text(source).unwrap_or("");
                    let inner = txt.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                    if inner == "drizzle-orm"
                        || inner.starts_with("drizzle-orm/")
                        || inner == "drizzle-zod"
                    {
                        flags.has_drizzle_import = true;
                    }
                    if inner == "zod" {
                        flags.has_zod_import = true;
                    }
                }
            }
            "call_expression" => {
                if let Some(func) = n.child_by_field_name("function") {
                    let name = func.utf8_text(source).unwrap_or("");
                    if TABLE_FNS.contains(&name) {
                        flags.has_table_call = true;
                    }
                    if name == "createInsertSchema" || name == "createSelectSchema" {
                        flags.uses_generator = true;
                    }
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
    flags
}

fn is_z_object(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    obj.utf8_text(source).unwrap_or("") == "z" && prop.utf8_text(source).unwrap_or("") == "object"
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let flags = collect_file_flags(node, source);
    if !flags.has_drizzle_import || !flags.has_zod_import || !flags.has_table_call || flags.uses_generator {
        return;
    }
    // Walk again, find every z.object(...) call.
    let mut stack: Vec<tree_sitter::Node> = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression" && is_z_object(n, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &n,
                super::META.id,
                "Manual `z.object({})` in a Drizzle schema file likely duplicates column definitions — use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` instead.".into(),
                Severity::Warning,
            ));
        }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
