//! ts-no-restricted-types backend — flag commonly banned types in type
//! annotation positions.
//!
//! Default banned types (mirrors typescript-eslint defaults):
//! - `{}` (empty object type) — use `object` or `Record<string, unknown>`
//! - `Function` — use a specific function type like `() => void`
//! - `Object` — use `object` or `Record<string, unknown>`

use crate::diagnostic::{Diagnostic, Severity};

/// Banned type identifiers and their replacement message.
const BANNED_TYPES: &[(&str, &str)] = &[
    ("Function", "Use a specific function type like `() => void` instead of `Function`."),
    ("Object", "Use `object` or `Record<string, unknown>` instead of `Object`."),
    ("{}",  "Use `object` or `Record<string, unknown>` instead of `{}`."),
];

/// True when `node` sits inside a type-annotation context.
fn in_type_context(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "type_annotation" | "type_alias_declaration" | "extends_clause"
            | "implements_clause" | "as_expression" | "satisfies_expression"
            | "generic_type" | "union_type" | "intersection_type"
            | "type_arguments" | "type_parameters" | "parenthesized_type"
            | "array_type" | "readonly_type" | "return_type"
            | "constraint" | "default_type" => return true,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Check type_identifier nodes for banned names (Function, Object)
    if node.kind() == "type_identifier" {
        let Ok(name) = node.utf8_text(source) else {
            return;
        };
        if let Some(&(_, msg)) = BANNED_TYPES.iter().find(|&&(t, _)| t == name)
            && in_type_context(node) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "ts-no-restricted-types".into(),
                    message: msg.to_string(),
                    severity: Severity::Warning,
                });
            }
        return;
    }

    // Check for empty object type `{}`
    if node.kind() == "object_type"
        && node.named_child_count() == 0 && in_type_context(node) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-no-restricted-types".into(),
                message: "Use `object` or `Record<string, unknown>` instead of `{}`.".into(),
                severity: Severity::Warning,
            });
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_function_type() {
        let d = run_on("const f: Function = () => {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Function"));
    }

    #[test]
    fn flags_object_type() {
        let d = run_on("const o: Object = {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object"));
    }

    #[test]
    fn allows_specific_function_type() {
        assert!(run_on("const f: () => void = () => {};").is_empty());
    }

    #[test]
    fn allows_record_type() {
        assert!(run_on("const o: Record<string, unknown> = {};").is_empty());
    }
}
