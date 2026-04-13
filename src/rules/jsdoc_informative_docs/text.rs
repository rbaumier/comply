use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects JSDoc descriptions that merely repeat the name of the symbol.
/// Example:
/// ```
/// /** The foo function. */
/// function foo() {}
/// ```
fn find_uninformative_docs(source: &str) -> Vec<(usize, usize)> {
    let mut hits = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Single-line JSDoc: /** ... */
        if trimmed.starts_with("/**") && trimmed.ends_with("*/") {
            let comment_body = extract_jsdoc_body(trimmed);
            if let Some(name) = get_next_symbol_name(&lines, idx + 1)
                && is_trivial_description(&comment_body, &name) {
                    hits.push((idx, 0));
                }
        }
    }
    hits
}

fn extract_jsdoc_body(line: &str) -> String {
    let s = line.trim_start_matches("/**").trim_end_matches("*/").trim();
    s.to_lowercase()
}

fn get_next_symbol_name(lines: &[&str], from: usize) -> Option<String> {
    for line in lines.iter().take(lines.len().min(from + 3)).skip(from) {
        let t = line.trim();
        // function foo(
        if let Some(rest) = t.strip_prefix("function ") {
            return extract_ident(rest);
        }
        // const foo =, let foo =, var foo =
        for kw in &["const ", "let ", "var ", "export const ", "export let ", "export function "] {
            if let Some(rest) = t.strip_prefix(kw) {
                return extract_ident(rest);
            }
        }
        // class Foo
        if let Some(rest) = t.strip_prefix("class ") {
            return extract_ident(rest);
        }
        if let Some(rest) = t.strip_prefix("export class ") {
            return extract_ident(rest);
        }
        // method: foo(
        if let Some(paren) = t.find('(') {
            let candidate = t[..paren].trim();
            if !candidate.is_empty() && candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(candidate.to_lowercase());
            }
        }
    }
    None
}

fn extract_ident(s: &str) -> Option<String> {
    let ident: String = s.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident.to_lowercase())
    }
}

fn is_trivial_description(body: &str, name: &str) -> bool {
    if body.is_empty() || name.is_empty() {
        return false;
    }
    // "the foo function" / "foo" / "the foo" / "a foo" / "returns foo"
    let normalized = body
        .replace("the ", "")
        .replace("a ", "")
        .replace("an ", "")
        .replace("this ", "")
        .replace("function", "")
        .replace("method", "")
        .replace("class", "")
        .replace("variable", "")
        .replace(".", "")
        .trim()
        .to_lowercase();
    normalized == *name || normalized.is_empty()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (line, col) in find_uninformative_docs(ctx.source) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line + 1,
                column: col + 1,
                rule_id: "jsdoc-informative-docs".into(),
                message: "JSDoc description merely repeats the symbol name without adding useful information.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_trivial_doc() {
        let src = "/** The foo function. */\nfunction foo() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_informative_doc() {
        let src = "/** Calculates the total price including tax. */\nfunction calculateTotal() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_just_name() {
        let src = "/** foo */\nconst foo = 1;";
        assert_eq!(run(src).len(), 1);
    }
}
