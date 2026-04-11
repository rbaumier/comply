//! no-useless-react-setstate AST backend — `setX(x)` is a no-op.

use crate::diagnostic::{Diagnostic, Severity};

/// Collect (state_name, setter_name) pairs from `useState` declarations.
fn collect_state_pairs(source: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("useState") {
            continue;
        }
        if let Some(bracket_start) = trimmed.find('[')
            && let Some(bracket_end) = trimmed.find(']') {
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
    pairs
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let pairs = collect_state_pairs(text);
    if pairs.is_empty() {
        return;
    }

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("useState") {
            continue;
        }
        for (state, setter) in &pairs {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_setstate_with_own_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_pairs() {
        let src = r#"
const [name, setName] = useState("");
const [age, setAge] = useState(0);
setName(name);
setAge(age);
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_setter_with_different_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count + 1);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_with_new_value() {
        let src = r#"
const [name, setName] = useState("");
setName("hello");
"#;
        assert!(run_on(src).is_empty());
    }
}
