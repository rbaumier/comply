use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect function components with props type not wrapped in `Readonly<>`.
/// Matches:
///   function MyComponent(props: MyType)
///   function MyComponent({ x, y }: MyType)
///   const MyComponent = (props: MyType) =>
///   const MyComponent: React.FC<MyType> — skipped (FC already implies readonly)
fn check_non_readonly_props(line: &str) -> bool {
    let trimmed = line.trim();

    // Must look like a component: function name starts with uppercase
    let is_function_component = is_function_decl_component(trimmed);
    let is_arrow_component = is_arrow_component(trimmed);

    if !is_function_component && !is_arrow_component {
        return false;
    }

    // Find the params section `: Type` after the destructuring or `props`
    // Look for pattern `: SomeType)` or `: SomeType}` where SomeType is not Readonly<...>
    if let Some(colon_type) = extract_props_type(trimmed) {
        let type_name = colon_type.trim();
        // Skip if already Readonly
        if type_name.starts_with("Readonly<") {
            return false;
        }
        // Skip inline object types `{ ... }`
        if type_name.starts_with('{') {
            return false;
        }
        // Skip React.FC patterns
        if type_name.is_empty() {
            return false;
        }
        return true;
    }

    false
}

fn is_function_decl_component(line: &str) -> bool {
    let without_export = line
        .strip_prefix("export default ")
        .or_else(|| line.strip_prefix("export "))
        .unwrap_or(line);

    if let Some(rest) = without_export.strip_prefix("function ") {
        // Next word must start with uppercase
        let name_start = rest.trim_start();
        return name_start
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase());
    }
    false
}

fn is_arrow_component(line: &str) -> bool {
    let without_export = line
        .strip_prefix("export default ")
        .or_else(|| line.strip_prefix("export "))
        .unwrap_or(line);

    if let Some(rest) = without_export.strip_prefix("const ") {
        let name = rest.split([' ', ':', '=']).next().unwrap_or("");
        if name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        {
            // Must contain `=>` or `= (`
            return rest.contains("=>");
        }
    }
    false
}

/// Extract the type annotation from the props parameter.
/// Returns the type string if found, e.g., "MyProps" from `(props: MyProps)`.
fn extract_props_type(line: &str) -> Option<&str> {
    // Find opening paren of params
    let paren_pos = line.find('(')?;
    let after_paren = &line[paren_pos + 1..];

    // Find `: Type` pattern — skip destructured `{` and find the `: `
    // We need to find a `:` that's followed by a type name, not inside destructuring
    let mut depth = 0;
    let mut i = 0;
    let bytes = after_paren.as_bytes();

    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b':' if depth == 0 => {
                // Found the type annotation
                let type_start = i + 1;
                let rest = after_paren[type_start..].trim_start();
                // Extract until `)` or `=>`
                let end = rest.find(')').unwrap_or(rest.len());
                let type_str = rest[..end].trim();
                if !type_str.is_empty() {
                    return Some(type_str);
                }
            }
            b')' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if check_non_readonly_props(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-read-only-props".into(),
                    message: "Props type should be wrapped in `Readonly<>` to prevent mutation."
                        .into(),
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
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_function_component_without_readonly() {
        assert_eq!(run("function MyComponent(props: MyProps) {").len(), 1);
    }

    #[test]
    fn flags_destructured_props_without_readonly() {
        assert_eq!(
            run("function MyComponent({ name, age }: MyProps) {").len(),
            1
        );
    }

    #[test]
    fn allows_readonly_props() {
        assert!(run("function MyComponent(props: Readonly<MyProps>) {").is_empty());
    }

    #[test]
    fn allows_readonly_destructured() {
        assert!(run("function MyComponent({ name }: Readonly<MyProps>) {").is_empty());
    }

    #[test]
    fn ignores_non_component_functions() {
        assert!(run("function helper(data: MyType) {").is_empty());
    }

    #[test]
    fn flags_arrow_component() {
        assert_eq!(run("const MyComponent = (props: MyProps) => {").len(), 1);
    }
}
