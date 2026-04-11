use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Coercion functions that can be passed directly to `.map()` and similar.
const COERCION_FUNCTIONS: &[&str] = &["String", "Number", "BigInt", "Boolean", "Symbol"];

/// Detects `.map(x => Number(x))` and similar patterns where a wrapper
/// arrow/function is unnecessary — `.map(Number)` is equivalent.
///
/// Patterns detected:
/// - `(x) => Number(x)` / `x => Number(x)` inside `.map()`, `.filter()`, `.find()`, etc.
/// - `function(x) { return Number(x); }` inside callbacks
fn find_coercion_wrapper(line: &str) -> Option<String> {
    for &func in COERCION_FUNCTIONS {
        // Arrow: `=> Number(x)` where x is the single parameter
        // Match patterns like `.map(x => Number(x))` or `.map((x) => Number(x))`
        if let Some(msg) = check_arrow_coercion(line, func) {
            return Some(msg);
        }

        // Block body: `=> { return Number(x); }` or `function(x) { return Number(x); }`
        if let Some(msg) = check_block_coercion(line, func) {
            return Some(msg);
        }
    }
    None
}

/// Check for arrow expression coercion: `x => Number(x)` or `(x) => Number(x)`
fn check_arrow_coercion(line: &str, func: &str) -> Option<String> {
    // Find `=> Func(` pattern
    let arrow_func = format!("=> {func}(");
    let idx = line.find(&arrow_func)?;

    // Extract the argument inside Func(...)
    let after_open = idx + arrow_func.len();
    let rest = line.get(after_open..)?;
    let close = rest.find(')')?;
    let arg = rest[..close].trim();

    // arg must be a simple identifier
    if arg.is_empty() || !is_simple_ident(arg) {
        return None;
    }

    // Now find the parameter name before `=>`
    // Look backwards from the `=>` for the parameter
    let before_arrow = line[..idx].trim_end();
    let param = extract_single_param(before_arrow)?;

    if param == arg {
        Some(format!(
            "Prefer `{func}` directly over wrapping it in a function. Use `.map({func})` instead of `.map(x => {func}(x))`."
        ))
    } else {
        None
    }
}

/// Check for block body coercion: `{ return Number(x); }`
fn check_block_coercion(line: &str, func: &str) -> Option<String> {
    let return_pat = format!("return {func}(");
    let idx = line.find(&return_pat)?;

    // Extract arg
    let after_open = idx + return_pat.len();
    let rest = line.get(after_open..)?;
    let close = rest.find(')')?;
    let arg = rest[..close].trim();

    if arg.is_empty() || !is_simple_ident(arg) {
        return None;
    }

    // Look for parameter before `{` ... `return`
    // Find the `{` before `return`
    let before_return = &line[..idx];
    let brace = before_return.rfind('{')?;
    let before_brace = line[..brace].trim_end();

    // Should have `=>` or `)` before `{`
    // Look for parameter: either `=> {` or `function(param) {`
    if let Some(arrow_pos) = before_brace.rfind("=>") {
        let before_arrow = before_brace[..arrow_pos].trim_end();
        let param = extract_single_param(before_arrow)?;
        if param == arg {
            return Some(format!(
                "Prefer `{func}` directly over wrapping it in a function. Use `.map({func})` instead of `.map(x => {{ return {func}(x); }})`."
            ));
        }
    }

    None
}

fn is_simple_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Extract a single parameter name from before an `=>`.
/// Handles: `x`, `(x)`, `(x: Type)` (TS annotation).
fn extract_single_param(s: &str) -> Option<&str> {
    let s = s.trim_end();

    // `(x)` or `(x: string)` — unwrap parens
    if s.ends_with(')') {
        let open = s.rfind('(')?;
        let inner = s[open + 1..s.len() - 1].trim();
        // Strip type annotation: `x: string` -> `x`
        let param = inner.split(':').next()?.trim();
        // Must be a single param (no commas)
        if inner.contains(',') {
            return None;
        }
        if is_simple_ident(param) {
            return Some(param);
        }
        return None;
    }

    // bare `x` — last word
    let last_word = s
        .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .next()?;
    if is_simple_ident(last_word) {
        Some(last_word)
    } else {
        None
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = find_coercion_wrapper(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-native-coercion-functions".into(),
                    message: msg,
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
    fn flags_map_arrow_number() {
        let d = run("arr.map(x => Number(x))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number"));
    }

    #[test]
    fn flags_map_arrow_string_parens() {
        let d = run("arr.map((s) => String(s))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("String"));
    }

    #[test]
    fn flags_map_arrow_boolean() {
        let d = run("arr.filter(v => Boolean(v))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Boolean"));
    }

    #[test]
    fn flags_block_body_return() {
        let d = run("arr.map(x => { return Number(x); })");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_usage() {
        assert!(run("arr.map(Number)").is_empty());
    }

    #[test]
    fn allows_different_param() {
        assert!(run("arr.map(x => Number(y))").is_empty());
    }

    #[test]
    fn allows_multiple_args() {
        assert!(run("arr.map(x => Number(x, 10))").is_empty());
    }

    #[test]
    fn flags_bigint_coercion() {
        let d = run("items.map(v => BigInt(v))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("BigInt"));
    }

    #[test]
    fn allows_non_coercion_function() {
        assert!(run("arr.map(x => parseInt(x))").is_empty());
    }
}
