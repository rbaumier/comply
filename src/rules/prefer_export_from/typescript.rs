use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::{FxHashMap, FxHashSet};

/// Collect named imports as `local_name -> module_specifier` so a later pass
/// can recognise re-exports of the same local name.
///
/// `import { foo } from './m'`         → { "foo" -> "./m" }
/// `import { foo as bar } from './m'`  → { "bar" -> "./m" }  (local name is `bar`)
/// `import type { foo } from './m'`    → skipped (type-only imports don't
///                                       round-trip through `export { foo }`)
fn collect_named_imports(program: tree_sitter::Node<'_>, source: &[u8]) -> FxHashMap<String, String> {
    let mut map = FxHashMap::default();
    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        let Some(spec_node) = child.child_by_field_name("source") else {
            continue;
        };
        let Ok(raw) = std::str::from_utf8(&source[spec_node.byte_range()]) else {
            continue;
        };
        let specifier = raw
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();

        // Walk the import_clause → named_imports → import_specifier nodes.
        let mut ic = child.walk();
        for clause in child.children(&mut ic) {
            if clause.kind() != "import_clause" {
                continue;
            }
            let mut cc = clause.walk();
            for clause_child in clause.children(&mut cc) {
                if clause_child.kind() != "named_imports" {
                    continue;
                }
                let mut sc = clause_child.walk();
                for spec in clause_child.children(&mut sc) {
                    if spec.kind() != "import_specifier" {
                        continue;
                    }
                    // Local binding = alias if present, else name.
                    let local = spec
                        .child_by_field_name("alias")
                        .or_else(|| spec.child_by_field_name("name"));
                    let Some(local) = local else { continue };
                    let Ok(local_name) = std::str::from_utf8(&source[local.byte_range()]) else {
                        continue;
                    };
                    map.insert(local_name.to_string(), specifier.clone());
                }
            }
        }
    }
    map
}

/// Collect identifiers referenced in the program body outside of import and
/// export statements.  A symbol in this set cannot be converted to a re-export
/// because it is consumed locally.
fn collect_locally_used_identifiers(
    program: tree_sitter::Node<'_>,
    source: &[u8],
) -> FxHashSet<String> {
    fn visit(node: tree_sitter::Node<'_>, source: &[u8], out: &mut FxHashSet<String>) {
        if node.kind() == "identifier" {
            if let Ok(name) = std::str::from_utf8(&source[node.byte_range()]) {
                out.insert(name.to_string());
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, out);
        }
    }

    let mut used = FxHashSet::default();
    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        // Skip import_statement (declarations) and export_statement (re-exports).
        if child.kind() == "import_statement" || child.kind() == "export_statement" {
            continue;
        }
        visit(child, source, &mut used);
    }
    used
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Anchor on `program` so the import-collection pass runs once per file
    // and we can correlate exports against it.
    let imports = collect_named_imports(node, source);
    if imports.is_empty() {
        return;
    }
    let locally_used = collect_locally_used_identifiers(node, source);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }
        // Skip re-export-from forms like `export { foo } from './m'` —
        // they already use the preferred shape.
        if child.child_by_field_name("source").is_some() {
            continue;
        }

        // Find the `export_clause` child holding the named export specifiers.
        let mut ec = child.walk();
        for export_part in child.children(&mut ec) {
            if export_part.kind() != "export_clause" {
                continue;
            }
            let mut sc = export_part.walk();
            for spec in export_part.children(&mut sc) {
                if spec.kind() != "export_specifier" {
                    continue;
                }
                // Local binding being re-exported = the `name` field
                // (the value before `as`, or the bare identifier).
                let Some(name_node) = spec.child_by_field_name("name") else {
                    continue;
                };
                let Ok(local_name) = std::str::from_utf8(&source[name_node.byte_range()]) else {
                    continue;
                };
                if let Some(specifier) = imports.get(local_name) {
                    // Skip if the symbol is also used locally — converting to a
                    // re-export would remove the local binding.
                    if locally_used.contains(local_name) {
                        continue;
                    }
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &spec,
                        "prefer-export-from",
                        format!(
                            "Use `export {{ {local_name} }} from '{specifier}'` instead of \
                             importing then re-exporting `{local_name}`."
                        ),
                        Severity::Warning,
                    ));
                }
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

    #[test]
    fn no_fp_when_import_used_locally_and_exported() {
        // GammeSchema is imported, used locally (GammeSchema.parse), and also
        // exported — cannot be converted to a re-export.
        let src = "import { GammeSchema } from './gamme-schema';\nconst x = GammeSchema.parse({});\nexport { GammeSchema };";
        assert!(run_ts(src).is_empty());
    }

    #[test]
    fn no_fp_when_import_aliased_used_locally_and_exported() {
        let src = "import { foo as bar } from './m';\nconsole.log(bar);\nexport { bar };";
        assert!(run_ts(src).is_empty());
    }
}
