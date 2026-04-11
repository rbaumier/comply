//! react-no-unstable-nested-components text backend.
//!
//! Detects function/arrow declarations returning JSX nested inside another
//! component function. Uses brace-depth tracking: when we enter a component
//! (a function whose name starts with an uppercase letter or is assigned to
//! an uppercase identifier), any function/arrow inside it that returns JSX
//! is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if the line looks like a component-level function start
/// (named function with uppercase or const Foo = ...).
fn is_component_start(line: &str) -> bool {
    let t = line.trim();
    // `function Foo(` or `export function Foo(` or `export default function Foo(`
    if let Some(idx) = t.find("function ") {
        let after = &t[idx + 9..];
        let after = after.trim_start();
        return after.starts_with(|c: char| c.is_ascii_uppercase());
    }
    false
}

/// Returns true if the line defines an inner function/arrow that looks like
/// a component (returns JSX or is assigned to an uppercase name).
fn is_nested_component_def(line: &str) -> bool {
    let t = line.trim();

    // `const Foo = (` or `const Foo = function` or `let Foo =`
    if let Some(rest) = t
        .strip_prefix("const ")
        .or_else(|| t.strip_prefix("let "))
        .or_else(|| t.strip_prefix("var "))
        && t.contains('=')
    {
        let name = rest
            .trim_start()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        if name.starts_with(|c: char| c.is_ascii_uppercase())
            && (t.contains("=>") || t.contains("function"))
        {
            return true;
        }
    }

    // `function Foo(` nested inside another function
    if let Some(idx) = t.find("function ") {
        let after = t[idx + 9..].trim_start();
        if after.starts_with(|c: char| c.is_ascii_uppercase()) {
            return true;
        }
    }

    false
}

fn line_contains_jsx(line: &str) -> bool {
    let t = line.trim();
    // Heuristic: contains `<SomeComponent` or `<div` etc. followed by typical JSX
    t.contains("</>")
        || t.contains("</")
        || (t.contains("return (") && t.contains('<'))
        || (t.contains("return <") )
        || (t.contains("=> <"))
        || (t.contains("=> (") && t.contains('<'))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Track brace depth to know if we're inside a component.
        let mut component_depth: Option<i32> = None;
        let mut component_start_line: Option<usize> = None;
        let mut depth: i32 = 0;
        // Track nested function start line and its depth.
        let mut nested_fn_start: Option<(usize, i32)> = None;
        let mut nested_has_jsx = false;

        for (idx, line) in lines.iter().enumerate() {
            let open = line.chars().filter(|&c| c == '{').count() as i32;
            let close = line.chars().filter(|&c| c == '}').count() as i32;

            // Check if this line starts a top-level component
            if component_depth.is_none() && is_component_start(line) {
                component_depth = Some(depth);
                component_start_line = Some(idx);
            }

            depth += open;
            depth -= close;

            // We're inside a component
            if let Some(comp_d) = component_depth {
                // Check if the component has ended
                if depth <= comp_d {
                    component_depth = None;
                    component_start_line = None;
                    nested_fn_start = None;
                    nested_has_jsx = false;
                    continue;
                }

                // Look for nested function definitions that look like components.
                // Skip the line that started the component itself.
                if nested_fn_start.is_none()
                    && component_start_line != Some(idx)
                    && is_nested_component_def(line)
                {
                    nested_fn_start = Some((idx, depth - open));
                    nested_has_jsx = false;
                }

                // If we're tracking a nested function, look for JSX
                if nested_fn_start.is_some()
                    && line_contains_jsx(line)
                {
                    nested_has_jsx = true;
                }

                // If nested function ends, emit diagnostic if it had JSX
                if let Some((start_line, fn_depth)) = nested_fn_start
                    && depth <= fn_depth + 1 && nested_has_jsx && idx > start_line
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start_line + 1,
                        column: 1,
                        rule_id: "react-no-unstable-nested-components".into(),
                        message: "Do not define components during render. React will \
                                  see a new component type on every render and destroy \
                                  the entire subtree's DOM and state. Move it outside \
                                  the parent component."
                            .into(),
                        severity: Severity::Warning,
                    });
                    nested_fn_start = None;
                    nested_has_jsx = false;
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
    fn flags_nested_component() {
        let src = r#"
function ParentComponent() {
    const NestedComponent = () => {
        return <div>nested</div>;
    };
    return <NestedComponent />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_function_component() {
        let src = r#"
function ParentComponent() {
    function ChildComponent() {
        return <span>child</span>;
    }
    return <ChildComponent />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_top_level_component() {
        let src = r#"
function MyComponent() {
    return <div>hello</div>;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component_nested_function() {
        let src = r#"
function ParentComponent() {
    const handleClick = () => {
        console.log("clicked");
    };
    return <button onClick={handleClick}>click</button>;
}
"#;
        assert!(run(src).is_empty());
    }
}
