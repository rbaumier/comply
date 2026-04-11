use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_pascal_case(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Extract the name from an export statement, or None for type/interface/re-exports.
fn extract_export(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Skip re-exports and wildcard.
    if trimmed.contains(" from ") || trimmed.starts_with("export *") {
        return None;
    }

    // Skip type/interface exports.
    if trimmed.starts_with("export type ") || trimmed.starts_with("export interface ") {
        return None;
    }

    // `export default ...`
    if let Some(rest) = trimmed.strip_prefix("export default ") {
        let rest = rest.trim();
        let rest = rest.strip_prefix("async ").unwrap_or(rest);
        return extract_name_from_declaration(rest);
    }

    // `export ...`
    let rest = trimmed.strip_prefix("export ")?;
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    extract_name_from_declaration(rest)
}

fn extract_name_from_declaration(rest: &str) -> Option<String> {
    for keyword in &["function ", "class "] {
        if let Some(after) = rest.strip_prefix(keyword) {
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    for keyword in &["const ", "let ", "var "] {
        if let Some(after) = rest.strip_prefix(keyword) {
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    if let Some(after) = rest.strip_prefix("enum ") {
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path_str = ctx.path.to_string_lossy();
        if !path_str.ends_with(".tsx") && !path_str.ends_with(".jsx") {
            return Vec::new();
        }

        let mut component_exports: Vec<(String, usize)> = Vec::new();
        let mut non_component_exports: Vec<(String, usize)> = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(name) = extract_export(line) {
                if is_pascal_case(&name) {
                    component_exports.push((name, idx));
                } else {
                    non_component_exports.push((name, idx));
                }
            }
        }

        if component_exports.is_empty() || non_component_exports.is_empty() {
            return Vec::new();
        }

        non_component_exports
            .iter()
            .map(|(name, line_idx)| Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line_idx + 1,
                column: 1,
                rule_id: "react-refresh-only-export-components".into(),
                message: format!(
                    "Non-component export `{name}` alongside component exports breaks React Fast Refresh. Move it to a separate module."
                ),
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
        Check.check(&CheckCtx::for_test(Path::new("comp.tsx"), source))
    }

    #[test]
    fn flags_mixed_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export const helper = () => {};
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn allows_component_only_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export function AnotherComponent() { return <span />; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_type_exports_with_components() {
        let source = r#"
export type Props = { name: string };
export interface Config { debug: boolean }
export function MyComponent() { return <div />; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_non_tsx_files() {
        let source = r#"
export function MyComponent() { return <div />; }
export const helper = () => {};
"#;
        let d = Check.check(&CheckCtx::for_test(Path::new("util.ts"), source));
        assert!(d.is_empty());
    }
}
