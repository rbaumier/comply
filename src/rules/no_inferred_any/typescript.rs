//! no-inferred-any AST backend — detect likely untyped patterns that infer `any`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the rest of the line contains a type narrowing keyword.
fn has_type_narrowing(rest: &str) -> bool {
    let trimmed = rest.trim();
    trimmed.contains(" as ")
        || trimmed.starts_with("as ")
        || trimmed.contains(" satisfies ")
        || trimmed.starts_with("satisfies ")
        || trimmed.contains(": ")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    // Only flag .ts/.tsx files, not .js
    let ext = ctx
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext != "ts" && ext != "tsx" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Pattern: `const x: any =`
        if trimmed.contains(": any =") || trimmed.contains(": any;") {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-inferred-any".into(),
                message: "Explicit `any` annotation — use a concrete type or `unknown`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            continue;
        }

        // Pattern: `= JSON.parse(` without type narrowing
        if let Some(pos) = trimmed.find("JSON.parse(") {
            let rest = &trimmed[pos + 11..];
            if !has_type_narrowing(rest) && !has_type_narrowing(&trimmed[..pos]) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-inferred-any".into(),
                    message: "`JSON.parse()` returns `any` — add a type assertion or `satisfies` clause.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                continue;
            }
        }

        // Pattern: `= await response.json()` or `.json()` without type narrowing
        if let Some(pos) = trimmed.find(".json()") {
            let rest = &trimmed[pos + 7..];
            if !has_type_narrowing(rest) && !has_type_narrowing(&trimmed[..pos]) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-inferred-any".into(),
                    message: "`.json()` returns `any` — add a type assertion or `satisfies` clause.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_json_parse_without_type() {
        assert_eq!(run_on("const data = JSON.parse(raw);").len(), 1);
    }

    #[test]
    fn allows_json_parse_with_as() {
        assert!(run_on("const data = JSON.parse(raw) as Config;").is_empty());
    }

    #[test]
    fn flags_response_json_without_type() {
        assert_eq!(run_on("const data = await response.json();").len(), 1);
    }

    #[test]
    fn allows_response_json_with_satisfies() {
        assert!(run_on("const data = await response.json() satisfies User;").is_empty());
    }

    #[test]
    fn flags_explicit_any() {
        assert_eq!(run_on("const x: any = getValue();").len(), 1);
    }
}
