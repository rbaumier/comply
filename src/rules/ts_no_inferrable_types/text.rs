//! ts-no-inferrable-types backend — detect redundant type annotations on
//! variables initialized with literals.
//!
//! Catches: `: number = <number>`, `: string = "..."`, `: boolean = true/false`,
//! `: null = null`, `: undefined = undefined`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static RE_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":\s*number\s*=\s*[-+]?\d+\.?\d*\b").unwrap()
});
static RE_STRING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#":\s*string\s*=\s*["'`]"#).unwrap()
});
static RE_BOOLEAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":\s*boolean\s*=\s*(true|false)\b").unwrap()
});
static RE_NULL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":\s*null\s*=\s*null\b").unwrap()
});
static RE_UNDEFINED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":\s*undefined\s*=\s*undefined\b").unwrap()
});

const PATTERNS: &[(&str, &str)] = &[
    ("number", "RE_NUMBER"),
    ("string", "RE_STRING"),
    ("boolean", "RE_BOOLEAN"),
    ("null", "RE_NULL"),
    ("undefined", "RE_UNDEFINED"),
];

fn get_regex(key: &str) -> &'static Regex {
    match key {
        "RE_NUMBER" => &RE_NUMBER,
        "RE_STRING" => &RE_STRING,
        "RE_BOOLEAN" => &RE_BOOLEAN,
        "RE_NULL" => &RE_NULL,
        "RE_UNDEFINED" => &RE_UNDEFINED,
        _ => unreachable!(),
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for &(type_name, regex_key) in PATTERNS {
                let re = get_regex(regex_key);
                if let Some(m) = re.find(line) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: m.start() + 1,
                        rule_id: "ts-no-inferrable-types".into(),
                        message: format!(
                            "Type `{type_name}` is trivially inferred from the literal — \
                             remove the type annotation."
                        ),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_number_literal() {
        let diags = run("const x: number = 5;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`number`"));
    }

    #[test]
    fn flags_string_literal() {
        let diags = run(r#"const s: string = "hello";"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_boolean_literal() {
        let diags = run("const b: boolean = true;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_literal_init() {
        assert!(run("const x: number = getValue();").is_empty());
    }

    #[test]
    fn allows_different_type_and_value() {
        assert!(run("const x: string | undefined = getValue();").is_empty());
    }

    #[test]
    fn flags_null_literal() {
        let diags = run("const x: null = null;");
        assert_eq!(diags.len(), 1);
    }
}
