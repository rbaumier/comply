use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Collect `set*` names from `useState` declarations.
fn collect_setters(source: &str) -> Vec<String> {
    let mut setters = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Match: const [xxx, setXxx] = useState
        if !trimmed.contains("useState") {
            continue;
        }
        if let Some(bracket_start) = trimmed.find('[')
            && let Some(bracket_end) = trimmed.find(']') {
                let inside = &trimmed[bracket_start + 1..bracket_end];
                let parts: Vec<&str> = inside.split(',').collect();
                if parts.len() == 2 {
                    let setter = parts[1].trim();
                    if setter.starts_with("set") && setter.len() > 3 {
                        setters.push(setter.to_string());
                    }
                }
            }
    }
    setters
}

/// Track nesting: we consider a setter call "in body" if it's not inside
/// useEffect, useCallback, useMemo, or an arrow/function expression that
/// looks like an event handler.
fn find_body_setter_calls(source: &str, setters: &[String]) -> Vec<(usize, String)> {
    let mut results = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Track depth of "safe" scopes (useEffect, useCallback, event handlers)
    let mut safe_depth: Vec<i32> = Vec::new(); // stack of brace depths where safe scopes start
    let mut brace_depth: i32 = 0;
    let mut in_component = false;

    for (idx, &line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect component function start (simplified)
        if (trimmed.starts_with("function ")
            || trimmed.starts_with("const ")
            || trimmed.starts_with("export function ")
            || trimmed.starts_with("export default function "))
            && trimmed.contains('(')
        {
            // Heuristic: component names start with uppercase
            let is_component = trimmed
                .split_whitespace()
                .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
                .is_some();
            if is_component {
                in_component = true;
            }
        }

        // Track safe scopes
        if trimmed.contains("useEffect(")
            || trimmed.contains("useCallback(")
            || trimmed.contains("useMemo(")
            || trimmed.contains("useLayoutEffect(")
            || trimmed.contains("onChange")
            || trimmed.contains("onClick")
            || trimmed.contains("onSubmit")
            || trimmed.contains("onBlur")
            || trimmed.contains("onFocus")
            || trimmed.contains("handleClick")
            || trimmed.contains("handleChange")
            || trimmed.contains("handleSubmit")
        {
            safe_depth.push(brace_depth);
        }

        for ch in line.chars() {
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
                // Pop safe scopes that have closed
                while let Some(&safe_start) = safe_depth.last() {
                    if brace_depth <= safe_start {
                        safe_depth.pop();
                    } else {
                        break;
                    }
                }
            }
        }

        if !in_component {
            continue;
        }

        // If we're inside a safe scope, skip
        if !safe_depth.is_empty() {
            continue;
        }

        // Check if this line calls any setter directly
        for setter in setters {
            let call = format!("{setter}(");
            if trimmed.contains(&call) {
                // Make sure this isn't the declaration line
                if !trimmed.contains("useState") && !trimmed.contains(&"[".to_string()) {
                    results.push((idx + 1, setter.clone()));
                }
            }
        }
    }

    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let setters = collect_setters(ctx.source);
        if setters.is_empty() {
            return Vec::new();
        }

        find_body_setter_calls(ctx.source, &setters)
            .into_iter()
            .map(|(line, setter)| Diagnostic {
                path: ctx.path.to_path_buf(),
                line,
                column: 1,
                rule_id: "no-hook-setter-in-body".into(),
                message: format!(
                    "`{setter}()` called directly in component body — causes infinite re-renders. Move to `useEffect` or an event handler."
                ),
                severity: Severity::Error,
            })
            .collect()
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
    fn flags_setter_in_body() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  setCount(1);
  return <div />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_setter_in_use_effect() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(1);
  }, []);
  return <div />;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_setter_in_event_handler() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  const handleClick = () => {
    setCount(count + 1);
  };
  return <div onClick={handleClick} />;
}
"#;
        assert!(run(src).is_empty());
    }
}
