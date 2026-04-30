//! angular-no-topromise backend — flag deprecated `.toPromise()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const NEEDLE: &str = ".toPromise(";

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".toPromise("])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            let mut search = 0usize;
            while let Some(p) = line[search..].find(NEEDLE) {
                let col = search + p;
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "`.toPromise()` is deprecated — use `firstValueFrom(observable$)` (or `lastValueFrom`) from `rxjs`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
                search = col + NEEDLE.len();
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
    fn flags_topromise_call() {
        let src = "const v = await this.http.get('/x').toPromise();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_firstvaluefrom() {
        let src = "const v = await firstValueFrom(this.http.get('/x'));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_in_comments() {
        let src = "// .toPromise() is deprecated\nconst v = await firstValueFrom(x$);";
        assert!(run(src).is_empty());
    }
}
