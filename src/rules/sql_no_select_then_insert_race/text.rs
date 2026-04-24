use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn extract_from_table(upper: &str) -> Option<String> {
    let idx = upper.find(" FROM ")?;
    let after = &upper[idx + " FROM ".len()..];
    let mut name = String::new();
    for ch in after.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            name.push(ch);
        } else if name.is_empty() {
            continue;
        } else {
            break;
        }
    }
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn extract_insert_table(upper: &str) -> Option<String> {
    let idx = upper.find("INSERT INTO ")?;
    let after = &upper[idx + "INSERT INTO ".len()..];
    let mut name = String::new();
    for ch in after.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            name.push(ch);
        } else if name.is_empty() {
            continue;
        } else {
            break;
        }
    }
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper_source = ctx.source.to_ascii_uppercase();
        let lines: Vec<&str> = upper_source.lines().collect();
        // Window: if a SELECT FROM X is followed within 20 lines by INSERT INTO X
        // with no ON CONFLICT clause in the INSERT, flag it.
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("SELECT") || !line.contains(" FROM ") {
                continue;
            }
            let Some(select_table) = extract_from_table(line) else {
                continue;
            };
            let window_end = (i + 20).min(lines.len());
            for j in (i + 1)..window_end {
                let inner = lines[j];
                if !inner.contains("INSERT INTO ") {
                    continue;
                }
                let Some(insert_table) = extract_insert_table(inner) else {
                    continue;
                };
                if insert_table != select_table {
                    continue;
                }
                // Scan following lines for ON CONFLICT within same INSERT statement
                let tail_end = (j + 10).min(lines.len());
                let has_on_conflict = (j..tail_end)
                    .any(|k| lines[k].contains("ON CONFLICT"));
                if !has_on_conflict {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: j + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "SELECT then INSERT on `{select_table}` is a TOCTOU race — use `INSERT ... ON CONFLICT`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
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

    #[test]
    fn flags_select_then_insert_same_table() {
        let src = "const exists = await db.query(`SELECT id FROM user WHERE email = $1`, [e]);\nif (!exists) {\n  await db.query(`INSERT INTO user (email) VALUES ($1)`, [e]);\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_on_conflict() {
        let src = "const exists = await db.query(`SELECT id FROM user WHERE email = $1`, [e]);\nawait db.query(`INSERT INTO user (email) VALUES ($1) ON CONFLICT (email) DO NOTHING`, [e]);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_different_tables() {
        let src = "`SELECT id FROM user`;\n`INSERT INTO audit (x) VALUES (1)`;";
        assert!(run(src).is_empty());
    }
}
