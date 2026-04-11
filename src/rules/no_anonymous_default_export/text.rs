//! no-anonymous-default-export backend — flag `export default function() {}`
//! and `export default class {}` (anonymous default exports).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the line is an anonymous default export of a function or class.
///
/// Detects:
/// - `export default function() {`
/// - `export default function ({`    (destructured params)
/// - `export default function<T>(`   (generic)
/// - `export default class {`
/// - `export default class extends`
/// - `export default class implements`
///
/// Does NOT flag:
/// - `export default function foo() {`  (named)
/// - `export default class Foo {`       (named)
/// - `export default someVariable;`     (identifier)
/// - `export default 42;`               (literal)
fn is_anonymous_default_export(line: &str) -> bool {
    let trimmed = line.trim();
    let rest = match trimmed.strip_prefix("export") {
        Some(r) => r.trim_start(),
        None => return false,
    };
    let rest = match rest.strip_prefix("default") {
        Some(r) => r.trim_start(),
        None => return false,
    };

    // `export default function`
    if let Some(after_fn) = rest.strip_prefix("function") {
        // Could be `function*` for generators.
        let after_fn = after_fn.strip_prefix('*').unwrap_or(after_fn);
        let after_fn = after_fn.trim_start();
        // If the next char is `(` or `<` or `{` — it's anonymous.
        // If the next char is an identifier char — it's named.
        if after_fn.is_empty() {
            return true;
        }
        let first = after_fn.as_bytes()[0];
        return first == b'(' || first == b'<' || first == b'{';
    }

    // `export default class`
    if let Some(after_class) = rest.strip_prefix("class") {
        let after_class = after_class.trim_start();
        // If empty, `{`, `extends`, or `implements` → anonymous.
        if after_class.is_empty() {
            return true;
        }
        let first = after_class.as_bytes()[0];
        if first == b'{' || first == b'<' {
            return true;
        }
        // `class extends` or `class implements` without a name.
        if after_class.starts_with("extends") || after_class.starts_with("implements") {
            return true;
        }
        return false;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if is_anonymous_default_export(trimmed) {
                let kind = if trimmed.contains("function") {
                    "function"
                } else {
                    "class"
                };
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-anonymous-default-export".into(),
                    message: format!(
                        "Anonymous default export {kind} — give it a name for \
                         better stack traces and refactoring support."
                    ),
                    severity: Severity::Warning,
                });
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
    fn flags_anonymous_function() {
        let d = run("export default function() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("function"));
    }

    #[test]
    fn flags_anonymous_class() {
        let d = run("export default class {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn flags_anonymous_generator() {
        let d = run("export default function*() {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_anonymous_class_extends() {
        let d = run("export default class extends Base {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_named_function() {
        assert!(run("export default function myFn() {}").is_empty());
    }

    #[test]
    fn allows_named_class() {
        assert!(run("export default class MyClass {}").is_empty());
    }

    #[test]
    fn allows_identifier() {
        assert!(run("export default myVariable;").is_empty());
    }

    #[test]
    fn allows_literal() {
        assert!(run("export default 42;").is_empty());
    }

    #[test]
    fn skips_comments() {
        assert!(run("// export default function() {}").is_empty());
    }
}
