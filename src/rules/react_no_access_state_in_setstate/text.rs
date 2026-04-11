//! react-no-access-state-in-setstate text backend.
//!
//! Flags `this.state` inside `this.setState(...)` calls. Reading
//! `this.state` inside `setState` may yield stale values because React
//! batches state updates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Track when we're inside a setState call
        let mut in_setstate = false;
        let mut paren_depth: i32 = 0;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if !in_setstate {
                if let Some(pos) = trimmed.find("this.setState(") {
                    in_setstate = true;
                    paren_depth = 0;
                    // Check the rest of this line after `setState(`
                    let after = &trimmed[pos..];
                    for ch in after.chars() {
                        match ch {
                            '(' => paren_depth += 1,
                            ')' => {
                                paren_depth -= 1;
                                if paren_depth <= 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    // Check for this.state on the same line after setState(
                    let call_content = &trimmed[pos + 14..];
                    if call_content.contains("this.state") {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "react-no-access-state-in-setstate".into(),
                            message: "`this.state` inside `setState()` reads stale \
                                      state. Use the updater callback: \
                                      `setState(prev => ...)`."
                                .into(),
                            severity: Severity::Warning,
                        });
                    }
                    if paren_depth <= 0 {
                        in_setstate = false;
                    }
                }
            } else {
                // We're inside a multi-line setState call
                for ch in trimmed.chars() {
                    match ch {
                        '(' => paren_depth += 1,
                        ')' => {
                            paren_depth -= 1;
                            if paren_depth <= 0 {
                                in_setstate = false;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if trimmed.contains("this.state") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-no-access-state-in-setstate".into(),
                        message: "`this.state` inside `setState()` reads stale \
                                  state. Use the updater callback: \
                                  `setState(prev => ...)`."
                            .into(),
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
    fn flags_this_state_in_setstate() {
        let src = "this.setState({ count: this.state.count + 1 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiline_setstate() {
        let src = r#"
this.setState({
    count: this.state.count + 1,
    name: this.state.name,
});
"#;
        // Two lines with this.state
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_updater_callback() {
        let src = "this.setState(prev => ({ count: prev.count + 1 }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_setstate() {
        let src = "const x = this.state.count;";
        assert!(run(src).is_empty());
    }
}
