use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Collect (state_name, setter_name) pairs from `useState` declarations.
fn collect_state_pairs(source: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("useState") {
            continue;
        }
        if let Some(bracket_start) = trimmed.find('[') {
            if let Some(bracket_end) = trimmed.find(']') {
                let inside = &trimmed[bracket_start + 1..bracket_end];
                let parts: Vec<&str> = inside.split(',').collect();
                if parts.len() == 2 {
                    let state = parts[0].trim().to_string();
                    let setter = parts[1].trim().to_string();
                    if !state.is_empty() && setter.starts_with("set") {
                        pairs.push((state, setter));
                    }
                }
            }
        }
    }
    pairs
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let pairs = collect_state_pairs(ctx.source);
        if pairs.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            // Skip declaration lines
            if trimmed.contains("useState") {
                continue;
            }
            for (state, setter) in &pairs {
                // Match `setX(x)` — setter called with its own state
                let call = format!("{setter}({state})");
                if trimmed.contains(&call) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-useless-react-setstate".into(),
                        message: format!(
                            "`{setter}({state})` is a no-op — setting state to its current value."
                        ),
                        severity: Severity::Warning,
                    });
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
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_setstate_with_own_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count);
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_pairs() {
        let src = r#"
const [name, setName] = useState("");
const [age, setAge] = useState(0);
setName(name);
setAge(age);
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_setter_with_different_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count + 1);
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_setter_with_new_value() {
        let src = r#"
const [name, setName] = useState("");
setName("hello");
"#;
        assert!(run(src).is_empty());
    }
}
