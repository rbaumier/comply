//! no-unsupported-node-builtins backend — compare each Node.js API usage
//! against the minimum Node version declared in the nearest `package.json`'s
//! `engines.node` field.
//!
//! Detection strategy: at the `program` root, resolve the minimum supported
//! Node major, then cursor-walk every descendant once. Three call shapes are
//! flagged:
//!   - bare identifier usage of a known global (e.g. `fetch`, `structuredClone`)
//!   - `<target>.<method>` where `<method>` is a modern Array/Iterator
//!     instance method (e.g. `arr.findLast(...)`)
//!   - `Object.<method>` / `Array.<method>` static method calls
//!
//! Declaration contexts (variable/function/class/parameter names) are skipped
//! so redeclaring a shim — `const fetch = require('node-fetch')` — doesn't
//! report the declaration itself.

use crate::diagnostic::{Diagnostic, Severity};

use super::{
    lookup_global, lookup_instance_method, lookup_static_method, min_node_major, parse_min_version,
};

/// True if the identifier lives in a declaration slot (variable name, param
/// name, function/class name). Prevents "shim" declarations from tripping
/// the rule on themselves.
fn is_declaration_name(node: tree_sitter::Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "variable_declarator"
        | "function_declaration"
        | "function"
        | "class_declaration"
        | "method_definition"
        | "required_parameter"
        | "optional_parameter"
        | "formal_parameters"
        | "arrow_function"
        | "rest_pattern"
        | "shorthand_property_identifier_pattern"
        | "property_identifier"
        | "labeled_statement" => {
            // If `parent.child_by_field_name("name")` points to us, skip.
            if parent
                .child_by_field_name("name")
                .is_some_and(|name| name.id() == node.id())
            {
                return true;
            }
            false
        }
        _ => false,
    }
}

/// Inspect a single descendant node.
fn check_node(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    min_version: u32,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match node.kind() {
        "identifier" => {
            if is_declaration_name(node) {
                return;
            }
            let Ok(text) = node.utf8_text(source) else {
                return;
            };
            if let Some(required) = lookup_global(text).filter(|&r| r > min_version) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    "no-unsupported-node-builtins",
                    format!(
                        "`{text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                    ),
                    Severity::Warning,
                ));
            }
        }
        "member_expression" => {
            let Some(prop) = node.child_by_field_name("property") else {
                return;
            };
            if prop.kind() != "property_identifier" {
                return;
            }
            let Ok(prop_text) = prop.utf8_text(source) else {
                return;
            };

            // Static method on `Object` / `Array`.
            if let Some(obj) = node
                .child_by_field_name("object")
                .filter(|o| o.kind() == "identifier")
            {
                let Ok(obj_text) = obj.utf8_text(source) else {
                    return;
                };
                if let Some(required) =
                    lookup_static_method(obj_text, prop_text).filter(|&r| r > min_version)
                {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &node,
                        "no-unsupported-node-builtins",
                        format!(
                            "`{obj_text}.{prop_text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                        ),
                        Severity::Warning,
                    ));
                    return;
                }
            }

            // Instance method — flagged regardless of receiver shape.
            if let Some(required) = lookup_instance_method(prop_text).filter(|&r| r > min_version) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &prop,
                    "no-unsupported-node-builtins",
                    format!(
                        "`.{prop_text}()` is not available in Node.js {min_version}; requires Node.js {required} or later."
                    ),
                    Severity::Warning,
                ));
            }
        }
        _ => {}
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let Some(min_version) = min_node_major(ctx) else {
        return;
    };

    let mut cursor = node.walk();
    let mut progressed = cursor.goto_first_child();
    while progressed {
        let child = cursor.node();
        if !(child.is_error() || child.is_missing()) {
            check_node(child, source, min_version, ctx, diagnostics);
        }

        if !(child.is_error() || child.is_missing()) && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                progressed = false;
                break;
            }
            if cursor.node().id() == node.id() {
                progressed = false;
                break;
            }
        }
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
        use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_with_engine(node_version: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        let pkg =
            format!(r#"{{"name":"t","version":"0.0.0","engines":{{"node":"{node_version}"}}}}"#);
        fs::write(dir.path().join("package.json"), pkg).unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &src_path, &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    fn setup_without_engine(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"t","version":"0.0.0"}"#,
        )
        .unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &src_path, &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    fn setup_without_package_json(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path: PathBuf = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &src_path, &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_fetch_below_18() {
        let d = setup_with_engine(">=16", "const res = fetch('http://example.com');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fetch"));
    }

    #[test]
    fn allows_fetch_at_18() {
        let d = setup_with_engine(">=18", "const res = fetch('http://example.com');");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_structured_clone_below_17() {
        let d = setup_with_engine(">=16", "const copy = structuredClone(obj);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("structuredClone"));
    }

    #[test]
    fn allows_structured_clone_at_17() {
        let d = setup_with_engine(">=17", "const copy = structuredClone(obj);");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_no_engine_field() {
        let d = setup_without_engine("const copy = structuredClone(obj);");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_no_package_json() {
        let d = setup_without_package_json("const copy = structuredClone(obj);");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_object_group_by_below_21() {
        let d = setup_with_engine(">=20", "Object.groupBy(arr, fn);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.groupBy"));
    }

    #[test]
    fn allows_object_group_by_at_21() {
        let d = setup_with_engine(">=21", "Object.groupBy(arr, fn);");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_find_last_below_18() {
        let d = setup_with_engine(">=16", "arr.findLast(x => x > 0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("findLast"));
    }

    #[test]
    fn flags_to_sorted_below_20() {
        let d = setup_with_engine(">=18", "arr.toSorted();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toSorted"));
    }

    #[test]
    fn allows_older_apis() {
        let d = setup_with_engine(">=16", "setTimeout(() => {}, 1000); arr.map(x => x);");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_local_shim_declaration() {
        let d = setup_with_engine(
            ">=16",
            "const fetch = require('node-fetch'); export { fetch };",
        );
        // The `const fetch = ...` declaration is skipped; the re-export is
        // still an identifier reference and correctly flagged.
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn does_not_flag_member_property_named_fetch() {
        let d = setup_with_engine(">=16", "obj.fetch();");
        assert!(d.is_empty());
    }

    #[test]
    fn parses_caret_range() {
        let d = setup_with_engine("^18.0.0", "const res = fetch('u');");
        assert!(d.is_empty());
    }

    #[test]
    fn parses_or_range_takes_minimum() {
        let d = setup_with_engine(">=14 || >=18", "const copy = structuredClone(obj);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_array_from_async_below_22() {
        let d = setup_with_engine(">=20", "Array.fromAsync(iter);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.fromAsync"));
    }

    #[test]
    fn parse_min_version_standalone() {
        assert_eq!(parse_min_version(">=16.0.0"), Some(16));
        assert_eq!(parse_min_version("^18"), Some(18));
        assert_eq!(parse_min_version("20.x"), Some(20));
        assert_eq!(parse_min_version(">=14 <22"), Some(14));
        assert_eq!(parse_min_version(">=14 || >=16"), Some(14));
        assert_eq!(parse_min_version(">=20 || >=18"), Some(18));
        assert_eq!(parse_min_version("garbage"), None);
    }
}
