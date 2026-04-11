use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MAX_DEPTH: usize = 3;

/// Control-flow keywords that open a nesting level when followed by `{`.
const CONTROL_FLOW_KEYWORDS: &[&str] = &[
    "if ", "if(", "for ", "for(", "while ", "while(", "switch ", "switch(", "try ",
];

/// Check if a line starts a control-flow block (keyword + opening brace).
fn is_control_flow_open(trimmed: &str) -> bool {
    for keyword in CONTROL_FLOW_KEYWORDS {
        if trimmed.starts_with(keyword) && trimmed.contains('{') {
            return true;
        }
        // Also handle `} else if (...) {` and `} else {`
        if trimmed.starts_with("} else") && trimmed.contains('{') {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut cf_depth: usize = 0;
        let mut brace_stack: Vec<bool> = Vec::new(); // true = control-flow brace, false = other

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            let is_cf = is_control_flow_open(trimmed);

            // Count opening and closing braces
            for ch in trimmed.chars() {
                match ch {
                    '{' => {
                        if is_cf {
                            cf_depth += 1;
                            brace_stack.push(true);

                            if cf_depth > MAX_DEPTH {
                                diagnostics.push(Diagnostic {
                                    path: ctx.path.to_path_buf(),
                                    line: idx + 1,
                                    column: 1,
                                    rule_id: "nested-control-flow".into(),
                                    message: format!(
                                        "Control-flow nesting depth is {} (max: {}).",
                                        cf_depth, MAX_DEPTH
                                    ),
                                    severity: Severity::Error,
                                });
                            }
                        } else {
                            brace_stack.push(false);
                        }
                    }
                    '}' => {
                        if let Some(was_cf) = brace_stack.pop()
                            && was_cf {
                                cf_depth = cf_depth.saturating_sub(1);
                            }
                    }
                    _ => {}
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn allows_shallow_nesting() {
        let src = r#"
function foo() {
    if (a) {
        if (b) {
            if (c) {
                doSomething();
            }
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deep_nesting() {
        let src = r#"
function foo() {
    if (a) {
        if (b) {
            if (c) {
                if (d) {
                    doSomething();
                }
            }
        }
    }
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("4"));
    }

    #[test]
    fn counts_mixed_control_flow() {
        let src = r#"
function bar() {
    for (const x of items) {
        while (condition) {
            try {
                if (check) {
                    boom();
                }
            }
        }
    }
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_non_control_flow_braces() {
        let src = r#"
function baz() {
    if (a) {
        const obj = { key: { nested: { deep: true } } };
    }
}
"#;
        assert!(run(src).is_empty());
    }
}
