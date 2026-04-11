use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_named_export(line: &str) -> bool {
    let trimmed = line.trim();
    // Match `export const`, `export let`, `export var`, `export function`, `export class`,
    // `export enum`, `export interface`, `export type`.
    // Exclude `export default` and `export {` (re-export block) and `export *`.
    if !trimmed.starts_with("export ") {
        return false;
    }
    let rest = &trimmed[7..];
    if rest.starts_with("default ")
        || rest.starts_with('{')
        || rest.starts_with('*')
    {
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut export_lines: Vec<usize> = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_named_export(line) {
                export_lines.push(idx + 1);
            }
        }
        if export_lines.len() <= 1 {
            return Vec::new();
        }
        // Flag all but the first.
        export_lines[1..]
            .iter()
            .map(|&line_num| Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line_num,
                column: 1,
                rule_id: "group-exports".into(),
                message: "Multiple named export declarations — consolidate into a single export block.".into(),
                severity: Severity::Warning,
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
    fn flags_multiple_exports() {
        let src = r#"export const a = 1;
export const b = 2;
export function foo() {}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_single_export() {
        let src = "export const a = 1;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_export_default() {
        let src = r#"export const a = 1;
export default function main() {}
"#;
        assert!(run(src).is_empty());
    }
}
