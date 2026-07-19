use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::source_imports_db_library;
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // A `.vue` `<script>` block is not parsed by oxc, so this Text backend
        // sees only raw lines. Mirror the gate the TS/JS backend applies
        // (`file_imports_db_library`): a `SELECT *` literal in a Vue component is
        // an executed query only when the component imports a DB/ORM library.
        // Demo/REPL components animate example SQL strings into the UI without any
        // such import, so they are display content, not queries to police. `.sql`
        // files are the query itself and always fire.
        if ctx.lang == Language::Vue && !source_imports_db_library(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if super::contains_select_star(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-select-star".into(),
                    message: "`SELECT *` wastes bandwidth — list columns explicitly so the API contract is visible and covering indexes can work.".into(),
                    severity: Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    fn run_vue(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), source))
    }

    #[test]
    fn flags() {
        assert_eq!(run("const q = `SELECT * FROM users`;").len(), 1);
    }

    #[test]
    fn flags_lowercase() {
        assert_eq!(run("const q = \"select * from users\";").len(), 1);
    }

    #[test]
    fn allows_explicit() {
        assert!(run("const q = `SELECT id, name FROM users`;").is_empty());
    }

    #[test]
    fn allows_jsdoc_comment_terminator() {
        // `*/` here closes a JSDoc comment describing a Vue Select component;
        // the `*` is a comment terminator, not a SQL wildcard (issue #4917).
        assert!(
            run("/** Whether or not to close the popover on date select */").is_empty()
        );
        assert!(
            run("  /** prevents the user from interacting with Select */").is_empty()
        );
    }

    #[test]
    fn still_flags_real_query_on_a_comment_line() {
        // A genuine wildcard followed by whitespace must still fire even when a
        // comment terminator appears later on the same line.
        assert_eq!(run("const q = `SELECT * FROM users`; // select */").len(), 1);
    }

    #[test]
    fn vue_repl_demo_example_strings_are_not_flagged_issue5424() {
        // electric-sql/electric PGliteReplDemo.vue: example SQL strings animated
        // into a demo REPL input. The component imports PGlite (not a recognized
        // DB driver), so the strings are display content, not executed queries.
        let src = r#"<script setup>
import { PGlite } from '@electric-sql/pglite'
const queries = ['SELECT version();', 'SELECT * FROM now();']
</script>"#;
        assert!(run_vue(src).is_empty());
    }

    #[test]
    fn vue_component_importing_db_library_still_flags() {
        // A Vue component that actually imports a DB/ORM library and embeds a
        // `SELECT *` query is policed like any TS/JS file.
        let src = r#"<script setup>
import postgres from 'postgres'
const sql = postgres()
const rows = await sql`SELECT * FROM users`
</script>"#;
        assert_eq!(run_vue(src).len(), 1);
    }
}
