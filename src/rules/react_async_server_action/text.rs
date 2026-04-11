//! react-async-server-action text backend.
//!
//! Flags functions with `"use server"` that are not `async`.
//! Two patterns:
//! 1. File-level `"use server"` directive — all exported functions must be async.
//! 2. Inline `"use server"` inside a function body — the function must be async.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_function_decl(line: &str) -> bool {
    let t = line.trim();
    (t.contains("function ") || t.contains("function(") || t.contains("=> {"))
        && !t.starts_with("//")
        && !t.starts_with('*')
}

fn is_async_function(line: &str) -> bool {
    let t = line.trim();
    t.contains("async ")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Check for file-level "use server" directive
        let file_level_use_server = lines.iter().enumerate().any(|(i, line)| {
            let t = line.trim();
            (t == "\"use server\"" || t == "'use server'" || t == "\"use server\";") && i < 5
        });

        if file_level_use_server {
            // All exported functions must be async
            for (idx, line) in lines.iter().enumerate() {
                let t = line.trim();
                if (t.starts_with("export ") || t.starts_with("export default "))
                    && is_function_decl(t)
                    && !is_async_function(t)
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-async-server-action".into(),
                        message: "Server action must be `async`. This file has \
                                  `\"use server\"` at the top — all exported \
                                  functions must be async."
                            .into(),
                        severity: Severity::Error,
                    });
                }
            }
        }

        // Check for inline "use server" inside function bodies
        for (idx, line) in lines.iter().enumerate() {
            let t = line.trim();
            if (t == "\"use server\"" || t == "'use server'" || t == "\"use server\";")
                && idx >= 5
            {
                // Look backwards for the enclosing function
                for back in (0..idx).rev() {
                    let prev = lines[back].trim();
                    if is_function_decl(prev) {
                        if !is_async_function(prev) {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: back + 1,
                                column: 1,
                                rule_id: "react-async-server-action".into(),
                                message: "Server action must be `async`. This function \
                                          contains `\"use server\"` but is not async."
                                    .into(),
                                severity: Severity::Error,
                            });
                        }
                        break;
                    }
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
        Check.check(&CheckCtx::for_test(Path::new("actions.ts"), source))
    }

    #[test]
    fn flags_non_async_with_file_directive() {
        let src = r#"
"use server"

export function createPost(data: FormData) {
    // ...
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_with_file_directive() {
        let src = r#"
"use server"

export async function createPost(data: FormData) {
    // ...
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_non_async_inline_use_server() {
        let src = r#"
function Component() {
    return <form>ok</form>;
}

function submitForm() {
    "use server"
    // do stuff
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_inline_use_server() {
        let src = r#"
function Component() {
    return <form>ok</form>;
}

async function submitForm() {
    "use server"
    // do stuff
}
"#;
        assert!(run(src).is_empty());
    }
}
