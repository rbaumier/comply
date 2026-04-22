//! file-name-differ-from-class backend.
//!
//! Collects every top-level `export_statement` in the file. If the module
//! exports exactly one binding and that binding is a named class or
//! function declaration (either `export default class Foo {}` /
//! `export default function foo() {}` or a single `export class`/`export
//! function`), compares the binding's identifier against the file stem.
//!
//! Matching is case-insensitive and tolerates any of PascalCase,
//! camelCase, kebab-case, snake_case — the alphanumeric letter sequences
//! just have to be equal once normalized (non-alphanumeric stripped,
//! lowercased).
//!
//! Common barrel / utility filenames (`index`, `types`, `constants`,
//! `utils`) are skipped — those are conventionally multi-export even
//! when they happen to expose a single binding today.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

const BARREL_STEMS: &[&str] = &["index", "types", "constants", "utils"];

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let Some(file_stem) = file_stem(ctx.path) else {
            return Vec::new();
        };
        if BARREL_STEMS.contains(&file_stem.to_ascii_lowercase().as_str()) {
            return Vec::new();
        }

        let source = ctx.source.as_bytes();
        let root = tree.root_node();

        // Walk top-level children. Count exports; if we see anything other
        // than a single named class/function export we bail.
        let mut primary: Option<(tree_sitter::Node, String)> = None;
        let mut export_count = 0usize;

        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() != "export_statement" {
                continue;
            }
            export_count += 1;
            if export_count > 1 {
                return Vec::new();
            }
            if let Some(named) = extract_named_declaration(child, source) {
                primary = Some(named);
            } else {
                // Export is not a class/function declaration we can map to
                // a filename (e.g. `export const x = 1`, `export { a, b }`,
                // `export * from ...`). Bail.
                return Vec::new();
            }
        }

        let Some((node, name)) = primary else {
            return Vec::new();
        };

        if names_match(&name, file_stem) {
            return Vec::new();
        }

        let pos = node.start_position();
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "file-name-differ-from-class".into(),
            message: format!(
                "File name `{file_stem}` should match its sole export `{name}` \
                 (rename the file to `{name}` in PascalCase, camelCase, kebab-case or snake_case)."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

/// Extract the declared name of a class or function exported by `node`
/// (an `export_statement`). Returns `None` for anything else — re-exports,
/// variable exports, anonymous default exports, `export * from ...`.
fn extract_named_declaration<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<(tree_sitter::Node<'a>, String)> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_declaration"
            | "class"
            | "function_declaration"
            | "generator_function_declaration" => {
                let name_node = child.child_by_field_name("name")?;
                let name = name_node.utf8_text(source).ok()?.to_string();
                if name.is_empty() {
                    return None;
                }
                return Some((name_node, name));
            }
            _ => {}
        }
    }
    None
}

/// File stem without any extension. Returns `None` for paths without a
/// filename.
fn file_stem(path: &std::path::Path) -> Option<&str> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| {
            // Strip every trailing `.xxx` so `Foo.d.ts` → `Foo`.
            let mut stem = name;
            while let Some(dot) = stem.rfind('.') {
                stem = &stem[..dot];
            }
            stem
        })
}

/// Case-insensitive alphanumeric equality — accepts PascalCase, camelCase,
/// kebab-case, snake_case variants of the same letter sequence.
fn names_match(export_name: &str, file_stem: &str) -> bool {
    normalize(export_name) == normalize(file_stem)
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_default_class_mismatch() {
        let d = run_on("export default class UserService {}", "utils_helpers.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("UserService"));
    }

    #[test]
    fn flags_named_class_mismatch() {
        let d = run_on("export class UserService {}", "billing.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_default_function_mismatch() {
        let d = run_on("export default function parseDate() {}", "helpers.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("parseDate"));
    }

    #[test]
    fn allows_pascal_case_match() {
        assert!(run_on("export default class UserService {}", "UserService.ts").is_empty());
    }

    #[test]
    fn allows_kebab_case_match() {
        assert!(run_on("export default class UserService {}", "user-service.ts").is_empty());
    }

    #[test]
    fn allows_snake_case_match() {
        assert!(run_on("export default class UserService {}", "user_service.ts").is_empty());
    }

    #[test]
    fn allows_camel_case_match() {
        assert!(run_on("export default function parseDate() {}", "parseDate.ts").is_empty());
    }

    #[test]
    fn allows_tsx_extension() {
        assert!(run_on("export default class Widget {}", "Widget.tsx").is_empty());
    }

    #[test]
    fn skips_index_file() {
        assert!(run_on("export default class UserService {}", "index.ts").is_empty());
    }

    #[test]
    fn skips_utils_file() {
        assert!(run_on("export default class UserService {}", "utils.ts").is_empty());
    }

    #[test]
    fn skips_types_file() {
        assert!(run_on("export default class UserService {}", "types.ts").is_empty());
    }

    #[test]
    fn skips_constants_file() {
        assert!(run_on("export default class UserService {}", "constants.ts").is_empty());
    }

    #[test]
    fn skips_multi_export_file() {
        let src = "export class UserService {}\nexport class Billing {}";
        assert!(run_on(src, "billing.ts").is_empty());
    }

    #[test]
    fn skips_variable_export() {
        assert!(run_on("export const foo = 1;", "bar.ts").is_empty());
    }

    #[test]
    fn skips_re_export() {
        assert!(run_on("export { Foo } from './foo';", "bar.ts").is_empty());
    }

    #[test]
    fn skips_anonymous_default() {
        assert!(run_on("export default class {}", "anything.ts").is_empty());
    }

    #[test]
    fn skips_default_identifier() {
        // `export default foo;` — nothing to name-check against.
        assert!(run_on("const foo = 1;\nexport default foo;", "bar.ts").is_empty());
    }

    #[test]
    fn allows_d_ts_stripping() {
        // `.d.ts` files: stem is the part before `.d.ts`.
        assert!(run_on("export default class Foo {}", "Foo.d.ts").is_empty());
    }
}
