use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper_source = ctx.source.to_ascii_uppercase();
        let lines: Vec<&str> = upper_source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            // Detect bare UNION (not UNION ALL).
            let Some(pos) = line.find("UNION") else {
                continue;
            };
            let after = &line[pos + "UNION".len()..];
            let rest_trim = after.trim_start();
            if rest_trim.starts_with("ALL") {
                continue;
            }
            // Heuristic: flag only when a nearby SELECT (±10 lines) mentions
            // an `id` column — a proxy for a primary key guaranteeing unique rows.
            let start = idx.saturating_sub(10);
            let end = (idx + 10).min(lines.len());
            let window: String = lines[start..end].join(" ");
            let mentions_id = window.contains("SELECT ID")
                || window.contains(" ID,")
                || window.contains(" ID ")
                || window.contains(".ID");
            if !mentions_id {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Both sides select a primary key — use `UNION ALL` to skip the dedup sort.".into(),
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
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_union_with_ids() {
        let src = "`SELECT id, name FROM a UNION SELECT id, name FROM b`";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_union_all() {
        let src = "`SELECT id, name FROM a UNION ALL SELECT id, name FROM b`";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_union_without_id_context() {
        let src = "`SELECT label FROM a UNION SELECT label FROM b`";
        assert!(run(src).is_empty());
    }
}
