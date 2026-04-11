use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Known CSS/style extensions that are legitimate side-effect imports.
const STYLE_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".sass", ".less", ".styl", ".stylus", ".pcss", ".postcss",
];

/// Check if the import source is a known style/CSS import.
fn is_style_import(source: &str) -> bool {
    STYLE_EXTENSIONS.iter().any(|ext| source.ends_with(ext))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Match bare `import 'foo';` or `import "foo";` — no specifiers.
            if !trimmed.starts_with("import ") {
                continue;
            }

            let rest = &trimmed[7..];

            // If the rest starts with a quote, it's a side-effect import.
            let quote = if rest.starts_with('\'') {
                '\''
            } else if rest.starts_with('"') {
                '"'
            } else {
                continue;
            };

            // Extract the source string.
            let after_quote = &rest[1..];
            if let Some(end_idx) = after_quote.find(quote) {
                let source = &after_quote[..end_idx];
                if !is_style_import(source) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-unassigned-import".into(),
                        message: format!(
                            "Side-effect import `{}` — imported module should be assigned.",
                            source
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
    fn flags_side_effect_import() {
        let src = "import 'polyfill';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("polyfill"));
    }

    #[test]
    fn allows_css_import() {
        let src = "import './styles.css';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_scss_import() {
        let src = "import './styles.scss';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_named_import() {
        let src = "import { foo } from 'bar';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_double_quoted_side_effect() {
        let src = r#"import "reflect-metadata";"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
