//! react-no-this-in-sfc text backend.
//!
//! Detects `this.` inside functional components. Functional components
//! use hooks, not `this.state` / `this.props`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if the line starts a functional component definition.
fn is_functional_component_start(line: &str) -> bool {
    let t = line.trim();

    // `function Foo(` or `export function Foo(` or `export default function Foo(`
    if let Some(idx) = t.find("function ") {
        let after = &t[idx + 9..];
        let after = after.trim_start();
        if after.starts_with(|c: char| c.is_ascii_uppercase()) {
            return true;
        }
    }

    // `const Foo = (...)  =>` or `const Foo = function`
    if (t.starts_with("const ") || t.starts_with("export const ")) && t.contains('=') {
        let eq_pos = t.find('=').unwrap_or(0);
        let before_eq = &t[..eq_pos];
        let name = before_eq
            .split_whitespace()
            .last()
            .unwrap_or("");
        if name.starts_with(|c: char| c.is_ascii_uppercase())
            && (t.contains("=>") || t.contains("function"))
        {
            return true;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut in_component = false;
        let mut depth: i32 = 0;
        let mut component_depth: i32 = 0;
        // Track if any JSX is found (confirms it's a component, not a plain function)
        let mut has_jsx = false;
        let mut this_lines: Vec<usize> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let open = trimmed.chars().filter(|&c| c == '{').count() as i32;
            let close = trimmed.chars().filter(|&c| c == '}').count() as i32;

            if !in_component && is_functional_component_start(trimmed) {
                in_component = true;
                component_depth = depth;
                has_jsx = false;
                this_lines.clear();
            }

            depth += open;
            depth -= close;

            if in_component {
                if trimmed.contains('<') && (trimmed.contains("/>") || trimmed.contains("</")) {
                    has_jsx = true;
                }
                if trimmed.contains("return") && trimmed.contains('<') {
                    has_jsx = true;
                }

                // Detect `this.` but not in comments or strings (simple heuristic)
                if trimmed.contains("this.") && !trimmed.starts_with("//") && !trimmed.starts_with('*') {
                    this_lines.push(idx);
                }

                if depth <= component_depth {
                    // Component ended. Emit diagnostics if JSX was found.
                    if has_jsx {
                        for &line_idx in &this_lines {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: line_idx + 1,
                                column: 1,
                                rule_id: "react-no-this-in-sfc".into(),
                                message: "`this` has no meaning in a functional component. \
                                          Use hooks instead."
                                    .into(),
                                severity: Severity::Error,
                            });
                        }
                    }
                    in_component = false;
                    this_lines.clear();
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
    fn flags_this_in_functional_component() {
        let src = r#"
function MyComponent() {
    const value = this.props.name;
    return <div>{value}</div>;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_this_state_in_functional() {
        let src = r#"
function Counter() {
    const count = this.state.count;
    return <span>{count}</span>;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_this_in_class_component() {
        // No JSX returned via function keyword + uppercase — but it's a class method
        let src = r#"
class MyComponent extends React.Component {
    render() {
        return <div>{this.props.name}</div>;
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_functional_without_this() {
        let src = r#"
function MyComponent({ name }) {
    return <div>{name}</div>;
}
"#;
        assert!(run(src).is_empty());
    }
}
