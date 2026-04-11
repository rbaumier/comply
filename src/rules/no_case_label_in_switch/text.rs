use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect label statements inside switch blocks.
/// A label looks like `identifier:` on its own line inside a switch, but is
/// not `case ...:` or `default:`.
fn find_labels_in_switch(source: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let mut flagged: Vec<usize> = Vec::new();
    let mut switch_depth: i32 = 0;
    let mut in_switch = false;

    for (idx, &line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Track switch entry
        if trimmed.contains("switch") && trimmed.contains('(') {
            in_switch = true;
        }

        // Track brace depth within switch
        if in_switch {
            for ch in trimmed.chars() {
                if ch == '{' {
                    switch_depth += 1;
                } else if ch == '}' {
                    switch_depth -= 1;
                    if switch_depth <= 0 {
                        in_switch = false;
                        switch_depth = 0;
                    }
                }
            }
        }

        if !in_switch || switch_depth <= 0 {
            continue;
        }

        // Skip `case ...:` and `default:`
        if trimmed.starts_with("case ") || trimmed == "default:" {
            continue;
        }

        // Check for label pattern: `identifier:` (word chars followed by colon)
        // Must be the entire statement (possibly with trailing whitespace/comment)
        if is_label_statement(trimmed) {
            flagged.push(idx);
        }
    }

    flagged
}

/// Check if a trimmed line is a label statement: `identifier:` possibly with
/// trailing content (the labeled statement).
fn is_label_statement(trimmed: &str) -> bool {
    // Find the first colon
    let colon_pos = match trimmed.find(':') {
        Some(p) => p,
        None => return false,
    };

    if colon_pos == 0 {
        return false;
    }

    let before = &trimmed[..colon_pos];

    // The part before the colon must be a simple identifier (no spaces, no operators)
    if !before
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    {
        return false;
    }

    // Must start with a letter or underscore or $
    let first = before.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' && first != '$' {
        return false;
    }

    // Skip common non-label patterns
    // `default:` is handled above, but double-check
    if before == "default" {
        return false;
    }

    // Skip object-like patterns (lines inside objects/types with `key: value`)
    // Heuristic: if after the colon there's a value that looks like a property value, skip
    let after = trimmed[colon_pos + 1..].trim();
    // Labels are followed by a statement or nothing. Property values have commas.
    // But we want to catch `foo:` even with a statement after it.
    // The key signal: inside a switch, a bare `word:` that isn't `case` or `default` is suspect.

    // Skip if the before part is a common keyword that takes a colon in TS types
    if before == "type" || before == "interface" || before == "readonly" {
        return false;
    }

    // If after colon is empty or starts with a statement keyword, it's a label
    if after.is_empty()
        || after.starts_with("//")
        || after.starts_with("break")
        || after.starts_with("continue")
        || after.starts_with("return")
        || after.starts_with("console")
        || after.starts_with("if")
        || after.starts_with("for")
        || after.starts_with("while")
        || after.starts_with('{')
    {
        return true;
    }

    // Conservative: also flag if the "identifier" contains common label-like names
    // that wouldn't be valid case expressions
    // But to avoid false positives on object literals, only flag if the identifier
    // is a single word that doesn't look like a property name
    // For safety, flag it - the user asked us to flag any `word:` that isn't case/default
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        find_labels_in_switch(ctx.source)
            .into_iter()
            .map(|line_idx| Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line_idx + 1,
                column: 1,
                rule_id: "no-case-label-in-switch".into(),
                message: "Label inside switch statement \u{2014} this is a JS label, not a case branch. Use `case <value>:` instead.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_label_in_switch() {
        let src = r#"
switch (action) {
    case "run":
        break;
    stop:
        console.log("stopped");
        break;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_labels() {
        let src = r#"
switch (x) {
    case 1:
        break;
    foo:
        break;
    bar:
        break;
}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_case_and_default() {
        let src = r#"
switch (x) {
    case "a":
        break;
    case "b":
        break;
    default:
        break;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_labels_outside_switch() {
        let src = r#"
myLabel:
for (let i = 0; i < 10; i++) {
    break myLabel;
}
"#;
        assert!(run(src).is_empty());
    }
}
