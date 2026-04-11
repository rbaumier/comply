//! react-no-constructed-context-values text backend.
//!
//! Flags `<Provider value={{ ... }}>` or `<Provider value={[ ... ]}>` —
//! inline object/array literals passed to a context Provider's `value`
//! prop create a new reference every render, forcing all consumers to
//! re-render.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            // Look for `Provider value={` followed by `{` or `[`
            if !trimmed.contains("Provider") {
                continue;
            }
            if let Some(pos) = trimmed.find("value={") {
                let after = &trimmed[pos + 7..];
                let first_non_ws = after.trim_start().chars().next();
                if first_non_ws == Some('{') || first_non_ws == Some('[') {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-no-constructed-context-values".into(),
                        message: "Context Provider `value` is an inline object/array — \
                                  a new reference is created every render, causing all \
                                  consumers to re-render. Memoize with `useMemo`."
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
    fn flags_inline_object() {
        let src = r#"<MyContext.Provider value={{ foo: 1 }}>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_array() {
        let src = r#"<ThemeProvider value={[theme, setTheme]}>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_memoized_value() {
        let src = r#"<MyContext.Provider value={memoizedValue}>"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_provider() {
        let src = r#"<Foo value={{ bar: 1 }} />"#;
        assert!(run(src).is_empty());
    }
}
