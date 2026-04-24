use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            let has_v4 =
                upper.contains("GEN_RANDOM_UUID()") || upper.contains("UUID_GENERATE_V4()");
            if !has_v4 {
                continue;
            }
            let mentions_pk = upper.contains("PRIMARY KEY")
                || upper.contains(" ID UUID")
                || upper.contains("\tID UUID")
                || upper.starts_with("ID UUID");
            if mentions_pk {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "UUIDv4 primary keys fragment B-tree indexes — use UUIDv7 or BIGINT IDENTITY instead."
                        .into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_gen_random_uuid_on_pk() {
        assert_eq!(
            run("const q = `id UUID PRIMARY KEY DEFAULT gen_random_uuid()`;").len(),
            1
        );
    }

    #[test]
    fn flags_uuid_generate_v4() {
        assert_eq!(
            run("const q = `id UUID PRIMARY KEY DEFAULT uuid_generate_v4()`;").len(),
            1
        );
    }

    #[test]
    fn allows_non_pk_uuid() {
        assert!(run("const q = `trace_id UUID DEFAULT gen_random_uuid()`;").is_empty());
    }
}
