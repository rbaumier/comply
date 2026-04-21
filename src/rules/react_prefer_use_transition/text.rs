use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if src.contains("useTransition") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if !t.contains("useState(false)") || !t.contains("const [") {
                continue;
            }
            if let Some(comma_pos) = t.find(", ") {
                let after = &t[comma_pos + 2..];
                if let Some(bracket) = after.find(']') {
                    let setter = after[..bracket].trim();
                    if !setter.is_empty()
                        && src.contains(&format!("{setter}(true)"))
                        && src.contains(&format!("{setter}(false)"))
                        && src.contains("await ")
                    {
                        diags.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: i + 1,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Replace manual `{setter}(true/false)` loading state with `useTransition`."
                            ),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_manual_loading_state() {
        let src = "const [loading, setLoading] = useState(false)\nasync function submit() { setLoading(true); await post(); setLoading(false) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_transition() {
        let src = "const [isPending, startTransition] = useTransition()\nconst [loading, setLoading] = useState(false)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_await() {
        let src = "const [loading, setLoading] = useState(false)\nfunction submit() { setLoading(true); post(); setLoading(false) }";
        assert!(run(src).is_empty());
    }
}
