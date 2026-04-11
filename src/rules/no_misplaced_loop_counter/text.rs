use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the identifier being compared in the condition part (second clause)
/// of a `for` loop. Looks for patterns like `i <`, `i >`, `i <=`, `i >=`, `i !=`.
fn extract_condition_var(condition: &str) -> Option<&str> {
    let cond = condition.trim();
    // Try to find a comparison operator and get the identifier before it
    for op in &["<=", ">=", "!=", "===", "!==", "<", ">"] {
        if let Some(pos) = cond.find(op) {
            let before = cond[..pos].trim();
            let ident = before
                .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                .next()?;
            if !ident.is_empty()
                && ident
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
            {
                return Some(ident);
            }
        }
    }
    None
}

/// Extract the identifier being modified in the update part (third clause)
/// of a `for` loop. Looks for `i++`, `++i`, `i--`, `--i`, `i +=`, `i -=`.
fn extract_update_var(update: &str) -> Option<&str> {
    let upd = update.trim().trim_end_matches(')');
    let upd = upd.trim();

    // Handle prefix ++/--
    if upd.starts_with("++") || upd.starts_with("--") {
        let ident = upd[2..].trim();
        if !ident.is_empty()
            && ident
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return Some(ident);
        }
    }

    // Handle postfix ++/--
    if upd.ends_with("++") || upd.ends_with("--") {
        let ident = upd[..upd.len() - 2].trim();
        if !ident.is_empty()
            && ident
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return Some(ident);
        }
    }

    // Handle += or -=
    for op in &["+=", "-="] {
        if let Some(pos) = upd.find(op) {
            let ident = upd[..pos].trim();
            if !ident.is_empty()
                && ident
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            {
                return Some(ident);
            }
        }
    }

    None
}

/// Parse a `for (init; cond; update)` line and check if condition and update
/// use different variables.
fn has_misplaced_counter(line: &str) -> bool {
    // Find `for` followed by `(`
    let Some(for_pos) = line.find("for") else {
        return false;
    };
    let after_for = &line[for_pos + 3..];
    let trimmed = after_for.trim_start();
    if !trimmed.starts_with('(') {
        return false;
    }

    let paren_content = &trimmed[1..];

    // Split on semicolons to get the three clauses
    // We need to handle nested parens
    let mut depth = 1i32;
    let mut semicolons = Vec::new();
    let mut close_paren = paren_content.len();
    for (i, b) in paren_content.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close_paren = i;
                    break;
                }
            }
            b';' if depth == 1 => semicolons.push(i),
            _ => {}
        }
    }

    if semicolons.len() < 2 {
        return false;
    }

    let condition = &paren_content[semicolons[0] + 1..semicolons[1]];
    let update = &paren_content[semicolons[1] + 1..close_paren];

    let cond_var = match extract_condition_var(condition) {
        Some(v) => v,
        None => return false,
    };
    let upd_var = match extract_update_var(update) {
        Some(v) => v,
        None => return false,
    };

    cond_var != upd_var
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_misplaced_counter(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-misplaced-loop-counter".into(),
                    message: "`for` loop condition and update use different variables — likely a copy-paste bug.".into(),
                    severity: Severity::Error,
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
    fn flags_different_vars() {
        assert_eq!(run("for (let i = 0; i < n; j++) {").len(), 1);
    }

    #[test]
    fn flags_prefix_increment_mismatch() {
        assert_eq!(run("for (let i = 0; i < 10; ++j) {").len(), 1);
    }

    #[test]
    fn flags_plus_equals_mismatch() {
        assert_eq!(run("for (let i = 0; i < n; j += 1) {").len(), 1);
    }

    #[test]
    fn allows_matching_vars() {
        assert!(run("for (let i = 0; i < n; i++) {").is_empty());
    }

    #[test]
    fn allows_matching_prefix() {
        assert!(run("for (let i = 0; i < 10; ++i) {").is_empty());
    }
}
