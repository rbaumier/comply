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

/// Minimum Node major version at which each global API became available as a
/// built-in global. Conservative: only the high-impact APIs people actually
/// trip over when bumping Node versions.
const GLOBAL_APIS: &[(&str, u32)] = &[
    ("AbortController", 15),
    ("AbortSignal", 15),
    ("BroadcastChannel", 15),
    ("atob", 16),
    ("btoa", 16),
    ("structuredClone", 17),
    ("fetch", 18),
    ("Blob", 18),
    ("FormData", 18),
    ("Headers", 18),
    ("Request", 18),
    ("Response", 18),
    ("CustomEvent", 19),
    ("File", 20),
    ("navigator", 21),
    ("WebSocket", 22),
];

/// Instance methods introduced on Array.prototype / typed array prototypes.
/// Flagged when seen as `<anything>.<method>(...)` — we don't try to prove
/// the receiver is an array.
const INSTANCE_METHODS: &[(&str, u32)] = &[
    ("findLast", 18),
    ("findLastIndex", 18),
    ("toSorted", 20),
    ("toReversed", 20),
    ("toSpliced", 20),
    ("with", 20),
    ("groupBy", 21),
];

/// Static methods on well-known constructors. Flagged only when the receiver
/// matches the expected constructor name.
const STATIC_METHODS: &[(&str, &str, u32)] = &[
    ("Object", "hasOwn", 16),
    ("Object", "groupBy", 21),
    ("Array", "fromAsync", 22),
];

fn lookup_global(name: &str) -> Option<u32> {
    GLOBAL_APIS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

fn lookup_instance_method(name: &str) -> Option<u32> {
    INSTANCE_METHODS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

fn lookup_static_method(obj: &str, prop: &str) -> Option<u32> {
    STATIC_METHODS
        .iter()
        .find(|(o, p, _)| *o == obj && *p == prop)
        .map(|(_, _, v)| *v)
}

/// Parse the minimum Node major version from an `engines.node` range string.
///
/// Supports the common shapes seen in the wild: `>=16`, `>=16.0.0`, `^18.0.0`,
/// `~20.1`, `16.x`, `>=14 <22`, and `||`-separated alternatives (we take the
/// minimum across alternatives). Anything we can't parse returns `None` and
/// the rule falls silent for that file.
fn parse_min_version(spec: &str) -> Option<u32> {
    // Split on `||` and take the minimum across alternatives — `||` means
    // "any of these ranges", so an app declaring `>=14 || >=16` runs on 14.
    let mut minimum: Option<u32> = None;
    for alt in spec.split("||") {
        if let Some(v) = parse_range_min(alt) {
            minimum = Some(minimum.map_or(v, |m| m.min(v)));
        }
    }
    minimum
}

/// Extract the first digit-run from a single range fragment, skipping common
/// operator prefixes. `>=16.0.0 <22` returns 16. `^18` returns 18.
fn parse_range_min(range: &str) -> Option<u32> {
    let bytes = range.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            return std::str::from_utf8(&bytes[start..i]).ok()?.parse().ok();
        }
        i += 1;
    }
    None
}

fn min_node_major(ctx: &crate::rules::backend::CheckCtx) -> Option<u32> {
    let pkg = ctx.project.nearest_package_json(ctx.path)?;
    let spec = pkg.engines.get("node")?;
    parse_min_version(spec)
}

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
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_ts_with_project_and_path;
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

        run_ts_with_project_and_path(source, &Check, &project, &src_path)
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

        run_ts_with_project_and_path(source, &Check, &project, &src_path)
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

        run_ts_with_project_and_path(source, &Check, &project, &src_path)
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
