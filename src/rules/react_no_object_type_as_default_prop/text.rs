//! react-no-object-type-as-default-prop text backend.
//!
//! Scans destructured function parameters for `= []`, `= {}`, or `= () =>`
//! defaults. These create a new reference on every render, defeating
//! `React.memo` and `useMemo` downstream.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static COMPONENT_DESTRUCTURE: LazyLock<Regex> = LazyLock::new(|| {
    // Match function/arrow component with destructured props:
    // `function Foo({`, `const Foo = ({`, `export default function Foo({`
    Regex::new(r"(?:function\s+[A-Z]\w*\s*\(\s*\{|(?:const|let)\s+[A-Z]\w*\s*=\s*(?:\([^)]*\)\s*=>|\([^)]*\)\s*:\s*\w+\s*=>|function)\s*\(\s*\{)").unwrap()
});

static OBJECT_DEFAULT: LazyLock<Regex> = LazyLock::new(|| {
    // Match destructured param with mutable default: `name = []`, `name = {}`, `name = () =>`
    Regex::new(r"\w+\s*=\s*(\[\s*\]|\{\s*\}|\([^)]*\)\s*=>)").unwrap()
});

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut in_component_params = false;
        let mut brace_depth: i32 = 0;

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Detect component function with destructured params
            if !in_component_params && COMPONENT_DESTRUCTURE.is_match(trimmed) {
                in_component_params = true;
                brace_depth = 0;
            }

            if in_component_params {
                for ch in trimmed.chars() {
                    match ch {
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        ')' if brace_depth <= 0 => {
                            in_component_params = false;
                            break;
                        }
                        _ => {}
                    }
                }

                if OBJECT_DEFAULT.is_match(trimmed) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-no-object-type-as-default-prop".into(),
                        message: "Object/array/function default prop creates a new \
                                  reference every render, breaking `React.memo`. Move \
                                  the default to a module-level constant."
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
    fn flags_empty_array_default() {
        let src = "function Foo({ items = [] }) { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_object_default() {
        let src = "function Bar({ config = {} }) { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_arrow_fn_default() {
        let src = "function Baz({ onClick = () => {} }) { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_primitive_default() {
        let src = "function Foo({ count = 0, name = 'hello' }) { return <div />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component() {
        let src = "function helper({ items = [] }) { return items; }";
        assert!(run(src).is_empty());
    }
}
