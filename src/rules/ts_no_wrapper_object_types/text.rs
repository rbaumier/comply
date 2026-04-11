//! ts-no-wrapper-object-types backend — detect wrapper object types
//! (`String`, `Number`, `Boolean`, `Object`, `Symbol`, `BigInt`) used in
//! type annotation positions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const WRAPPER_TYPES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Number", "number"),
    ("Boolean", "boolean"),
    ("Object", "object"),
    ("Symbol", "symbol"),
    ("BigInt", "bigint"),
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Look for type annotation contexts: `: Type`, `<Type>`, `as Type`
            for &(wrapper, preferred) in WRAPPER_TYPES {
                // Find all occurrences of the wrapper type in this line
                let mut search_from = 0;
                while let Some(pos) = line[search_from..].find(wrapper) {
                    let abs_pos = search_from + pos;
                    search_from = abs_pos + wrapper.len();

                    // Ensure it's a whole word (not part of a larger identifier)
                    if abs_pos > 0 {
                        let prev = line.as_bytes()[abs_pos - 1];
                        if prev.is_ascii_alphanumeric() || prev == b'_' {
                            continue;
                        }
                    }
                    if search_from < line.len() {
                        let next = line.as_bytes()[search_from];
                        if next.is_ascii_alphanumeric() || next == b'_' {
                            continue;
                        }
                    }

                    // Check if it's in a type annotation context:
                    // Look for `:` or `<` or `|` or `&` or `as ` before the wrapper,
                    // or `>` or `,` or `)` or `]` or `|` or `&` after.
                    let before = line[..abs_pos].trim_end();
                    let is_type_context = before.ends_with(':')
                        || before.ends_with('<')
                        || before.ends_with(',')
                        || before.ends_with('|')
                        || before.ends_with('&')
                        || before.ends_with("as")
                        || before.ends_with("extends")
                        || before.ends_with("implements");

                    if !is_type_context {
                        continue;
                    }

                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: abs_pos + 1,
                        rule_id: "ts-no-wrapper-object-types".into(),
                        message: format!(
                            "Use `{preferred}` instead of `{wrapper}` — \
                             the uppercase variant is the wrapper object type, \
                             not the primitive."
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
    fn flags_string_type() {
        let diags = run("const x: String = 'hello';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`string`"));
    }

    #[test]
    fn flags_number_type() {
        let diags = run("const x: Number = 5;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_boolean_type() {
        let diags = run("function f(x: Boolean): void {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_lowercase_primitives() {
        assert!(run("const x: string = 'hello';").is_empty());
        assert!(run("const x: number = 5;").is_empty());
    }

    #[test]
    fn flags_in_generic_position() {
        let diags = run("const x: Array<String> = [];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_runtime_usage() {
        assert!(run("const x = String(y);").is_empty());
    }
}
