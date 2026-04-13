use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashMap;

/// Extract named imports from a line like `import { a, b } from './m'`.
/// Returns (vec_of_names, specifier_string).
fn parse_named_import(line: &str) -> Option<(Vec<String>, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import") {
        return None;
    }

    let open = trimmed.find('{')?;
    let close = trimmed.find('}')?;
    if close <= open {
        return None;
    }

    // Must have `from` after the closing brace.
    let after_brace = &trimmed[close + 1..];
    if !after_brace.contains("from") {
        return None;
    }

    // Extract specifier (quoted string after `from`).
    let specifier = extract_quoted_after_from(after_brace)?;

    // Parse names between braces, handling `x as y` (take `y` — the local name).
    let names_str = &trimmed[open + 1..close];
    let names: Vec<String> = names_str
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            // Skip `type` imports — `import { type Foo } from …`
            let part = part.strip_prefix("type ").map_or(part, |rest| rest.trim());
            // `x as y` → local name is `y`
            if let Some(pos) = part.find(" as ") {
                Some(part[pos + 4..].trim().to_string())
            } else {
                Some(part.to_string())
            }
        })
        .collect();

    if names.is_empty() {
        return None;
    }
    Some((names, specifier))
}

/// Extract a quoted string after the `from` keyword.
fn extract_quoted_after_from(s: &str) -> Option<String> {
    let from_pos = s.find("from")?;
    let after = &s[from_pos + 4..];
    for delim in ['"', '\''] {
        if let Some(start) = after.find(delim) {
            let rest = &after[start + 1..];
            if let Some(end) = rest.find(delim) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

/// Check if a line is a bare re-export like `export { a, b };` (no `from`).
/// Returns the list of exported names.
fn parse_bare_export(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with("export") {
        return None;
    }
    let after_export = trimmed.strip_prefix("export")?.trim_start();
    if !after_export.starts_with('{') {
        return None;
    }
    // If it has `from`, it's already a re-export — skip.
    let close = after_export.find('}')?;
    let after_brace = &after_export[close + 1..];
    if after_brace.contains("from") {
        return None;
    }

    let names_str = &after_export[1..close];
    let names: Vec<String> = names_str
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            // `x as y` → the original local name is `x`
            if let Some(pos) = part.find(" as ") {
                Some(part[..pos].trim().to_string())
            } else {
                Some(part.to_string())
            }
        })
        .collect();

    if names.is_empty() {
        return None;
    }
    Some(names)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");

    // Phase 1: collect all named imports.
    let mut imports: HashMap<String, (String, usize)> = HashMap::new();
    for (idx, line) in src.lines().enumerate() {
        if let Some((names, specifier)) = parse_named_import(line) {
            for name in names {
                imports.insert(name, (specifier.clone(), idx + 1));
            }
        }
    }

    if imports.is_empty() {
        return;
    }

    // Phase 2: find bare re-exports that reference imported names.
    for (idx, line) in src.lines().enumerate() {
        let Some(export_names) = parse_bare_export(line) else {
            continue;
        };
        for name in &export_names {
            if let Some((specifier, _import_line)) = imports.get(name) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-export-from".into(),
                    message: format!(
                        "Use `export {{ {name} }} from '{specifier}'` instead of \
                         importing then re-exporting `{name}`."
                    ),
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_import_then_reexport() {
        let src = "import { foo } from './mod';\nexport { foo };";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("export { foo } from './mod'"));
    }

    #[test]
    fn flags_multiple_reexports() {
        let src = "import { a, b } from './m';\nexport { a, b };";
        let d = run_ts(src);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_direct_export_from() {
        assert!(run_ts("export { foo } from './mod';").is_empty());
    }

    #[test]
    fn allows_import_used_locally() {
        assert!(run_ts("import { foo } from './mod';\nconsole.log(foo);").is_empty());
    }

    #[test]
    fn allows_export_of_local() {
        assert!(run_ts("const bar = 1;\nexport { bar };").is_empty());
    }

    #[test]
    fn handles_renamed_import() {
        let src = "import { foo as bar } from './m';\nexport { bar };";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
    }
}
