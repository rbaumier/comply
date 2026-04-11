use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Collect variable names declared as arrays (`= []` or `: Array`).
fn array_vars(source: &str) -> Vec<String> {
    let mut vars = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Match `const/let/var name = []` or `const/let/var name: ... = []`
        for keyword in &["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(keyword) {
                let ident: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !ident.is_empty()
                    && (rest.contains("= []")
                        || rest.contains("= new Array")
                        || rest.contains(": Array"))
                {
                    vars.push(ident);
                }
            }
        }
    }
    vars
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let vars = array_vars(ctx.source);
        if vars.is_empty() {
            return diagnostics;
        }
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            for var in &vars {
                // arr["key"] = or arr['key'] =
                let bracket_double = format!("{var}[\"");
                let bracket_single = format!("{var}['");
                if (trimmed.contains(&bracket_double) || trimmed.contains(&bracket_single))
                    && trimmed.contains("=")
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-associative-arrays".into(),
                        message: format!("Array `{var}` is used as an associative array — use a Map or plain object instead."),
                        severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_bracket_string_key_assignment() {
        let src = r#"
const arr = [];
arr["key"] = 1;
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_quote_bracket_key() {
        let src = r#"
let items = [];
items['name'] = "hello";
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_numeric_index() {
        let src = r#"
const arr = [];
arr[0] = 1;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_array_bracket_access() {
        let src = r#"
const obj = {};
obj["key"] = 1;
"#;
        assert!(run(src).is_empty());
    }
}
