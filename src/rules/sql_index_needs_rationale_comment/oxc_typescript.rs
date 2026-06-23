//! sql-index-needs-rationale-comment — oxc backend for TS / JS / TSX.
//!
//! Walks StringLiteral and TemplateLiteral nodes, strips delimiters, and
//! delegates to `rust::check_string_content` for the actual detection.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

use super::rust::check_string_content;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Auto-generated ORM migration files (drizzle/payload `payload
        // migrate:create`, TypeORM, Knex/Umzug) emit their `CREATE INDEX`
        // statements verbatim and overwrite the file on every regeneration, so
        // a hand-added rationale comment cannot survive. Detected via the shared
        // `FileCtx` migration signal: a `migrations/` directory segment or an
        // exported `up`/`down` migration-hook pair.
        if ctx.file.is_migration_file() {
            return;
        }

        let min_rationale_words = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "min_rationale_words", ctx.lang);
        let lookback_lines = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "lookback_lines", ctx.lang);

        let (content, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => {
                (lit.value.as_str(), lit.span.start as usize)
            }
            AstKind::TemplateLiteral(tpl) => {
                // For template literals with multiple quasis, join them.
                // We only use the first quasi's raw text if there's one segment;
                // otherwise fall back to full raw text from source.
                if tpl.quasis.len() == 1 {
                    (tpl.quasis[0].value.raw.as_str(), tpl.span.start as usize)
                } else {
                    // Multi-segment: extract from source between backticks.
                    let start = tpl.span.start as usize + 1;
                    let end = tpl.span.end as usize;
                    let end = if end > 0 { end - 1 } else { end };
                    if let Some(slice) = ctx.source.get(start..end) {
                        (slice, tpl.span.start as usize)
                    } else {
                        return;
                    }
                }
            }
            _ => return,
        };

        // Cheap content pre-filter before the O(offset) line/column lookup:
        // skip the vast majority of string literals that can't be a CREATE INDEX.
        if !super::rust::content_has_create_index(content) {
            return;
        }
        let (line, col) = byte_offset_to_line_col(ctx.source, offset);
        // line is 1-based from byte_offset_to_line_col, but check_string_content
        // expects a 0-based row index (it adds +1 internally).
        diagnostics.extend(check_string_content(
            content,
            line.saturating_sub(1),
            col,
            ctx.path,
            min_rationale_words,
            lookback_lines,
        ));
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        let path = std::path::Path::new(path);
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            source,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    #[test]
    fn flags_create_index_without_comment() {
        let src = "const sql = `CREATE INDEX idx_foo ON bar(baz);`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_create_index_with_preceding_comment() {
        let src = "const sql = `-- Accelerates dashboard query for user_id\nCREATE INDEX idx_foo ON bar(baz);`;";
        assert!(run_on(src).is_empty());
    }

    // Regression for #4690: Payload CMS / Drizzle ORM emit migration files
    // (`payload migrate:create`) whose `CREATE INDEX` statements are generated
    // verbatim and overwritten on every regeneration — a rationale comment can
    // never survive. The shared migration signal (a `migrations/` segment plus
    // the exported `up`/`down` hook pair) exempts the whole file.
    #[test]
    fn ignores_create_index_in_generated_drizzle_migration_issue_4690() {
        let src = "import type { MigrateUpArgs, MigrateDownArgs } from '@payloadcms/db-postgres';\n\
            export async function up({ payload }: MigrateUpArgs): Promise<void> {\n\
            \x20 await payload.db.drizzle.execute(sql`\n\
            \x20   CREATE INDEX IF NOT EXISTS \"users_created_at_idx\" ON \"users\" USING btree (\"created_at\");\n\
            \x20 `)\n\
            }\n\
            export async function down({ payload }: MigrateDownArgs): Promise<void> {}\n";
        assert!(
            run_at(src, "test/database/migrations/20241219_161447.ts").is_empty(),
            "generated migration index DDL must not be flagged"
        );
    }

    // Negative space for #4690: hand-written application SQL with a `CREATE
    // INDEX` and no migration signal (not in a migrations/ dir, no up/down hook
    // pair) must STILL require a rationale comment.
    #[test]
    fn still_flags_create_index_in_regular_app_code_issue_4690() {
        let src = "const sql = `CREATE INDEX idx_foo ON bar(baz);`;";
        assert_eq!(run_at(src, "src/db/setup.ts").len(), 1);
    }

    // Regression for #892: `byte_offset_to_line_col` is O(offset), so calling
    // it for every string literal made this rule O(n²) — a 50k-literal file
    // took ~25s. The `content_has_create_index` pre-filter keeps it linear.
    // Generous wall-clock budget: post-fix this is tens of ms even in debug.
    #[test]
    fn many_non_sql_literals_stay_linear() {
        let mut src = String::from("const names = [\n");
        for i in 0..20_000 {
            src.push_str("  \"surname");
            src.push_str(&i.to_string());
            src.push_str("\",\n");
        }
        src.push_str("];\n");
        let start = std::time::Instant::now();
        assert!(run_on(&src).is_empty());
        assert!(
            start.elapsed().as_secs() < 5,
            "sql-index-needs-rationale-comment went quadratic: {:?}",
            start.elapsed()
        );
    }
}
