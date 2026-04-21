use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.contains("queryKey:") || !t.contains('[') {
                continue;
            }
            if let Some(bracket_start) = t.find("queryKey:") {
                let after = &t[bracket_start..];
                if let Some(arr_start) = after.find('[')
                    && let Some(arr_end) = after.find(']')
                {
                    let arr = &after[arr_start + 1..arr_end];
                    let has_string = arr.contains('\'') || arr.contains('"');
                    let parts: Vec<&str> = arr.split(',').collect();
                    let has_var = parts.iter().any(|p| {
                        let p = p.trim();
                        !p.is_empty() && !p.starts_with('\'') && !p.starts_with('"')
                    });
                    if has_string && has_var {
                        diags.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: i + 1,
                            column: line.find("queryKey").unwrap_or(0) + 1,
                            rule_id: super::META.id.into(),
                            message: "Extract dynamic `queryKey` to a key factory: `const keys = { detail: (id) => ['res', id] as const }`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_inline_dynamic_key() {
        assert_eq!(
            run("useQuery({ queryKey: ['todos', userId], queryFn: f })").len(),
            1
        );
    }

    #[test]
    fn allows_static_key() {
        assert!(run("useQuery({ queryKey: ['todos'], queryFn: f })").is_empty());
    }

    #[test]
    fn allows_factory() {
        assert!(
            run("useQuery({ queryKey: todoKeys.detail(userId), queryFn: f })").is_empty()
        );
    }
}
