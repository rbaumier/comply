use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `node` is the AST shape `import.meta.url` —
/// a `member_expression` whose object is the `import.meta` meta-property
/// and whose property name is `url`.
fn is_import_meta_url(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = node.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = node.child_by_field_name("property") else {
        return false;
    };
    let Ok(prop_text) = std::str::from_utf8(&source[prop.byte_range()]) else {
        return false;
    };
    if prop_text != "url" {
        return false;
    }
    // `import.meta` shows up as `meta_property` in the TS grammar.
    if obj.kind() == "meta_property" {
        return true;
    }
    // Fallback: textual match (covers older grammars / different node names).
    let Ok(obj_text) = std::str::from_utf8(&source[obj.byte_range()]) else {
        return false;
    };
    obj_text == "import.meta"
}

/// Match a 1-arg call where the callee is identifier `name` and the only
/// argument is `import.meta.url`. Returns the callee name on match.
fn is_call_to_with_import_meta_url<'a>(
    call: tree_sitter::Node<'a>,
    expected_callee_name: &str,
    source: &[u8],
) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    let callee_text = match func.kind() {
        "identifier" => std::str::from_utf8(&source[func.byte_range()]).ok(),
        // `path.dirname` — handled by the caller via `member_callee`.
        _ => None,
    };
    if callee_text != Some(expected_callee_name) {
        return false;
    }
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    single_argument(args, source).is_some_and(|arg| is_import_meta_url(arg, source))
}

/// Match `obj.method(import.meta.url)` where the member callee is exactly
/// `obj.method` (e.g. `path.dirname`). Returns true on match.
fn is_method_call_with_import_meta_url(
    call: tree_sitter::Node<'_>,
    expected_object: &str,
    expected_method: &str,
    source: &[u8],
) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    let obj_text = std::str::from_utf8(&source[obj.byte_range()]).unwrap_or("");
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if obj_text != expected_object || prop_text != expected_method {
        return false;
    }
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Some(arg) = single_argument(args, source) else {
        return false;
    };
    // The single argument must itself be a call to `fileURLToPath(import.meta.url)`.
    arg.kind() == "call_expression" && is_call_to_with_import_meta_url(arg, "fileURLToPath", source)
}

/// Return the only "real" argument inside an `arguments` node, ignoring
/// punctuation children (the `(`, `,`, `)` tokens are children too).
fn single_argument<'a>(
    args: tree_sitter::Node<'a>,
    _source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut found = None;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if found.is_some() {
            return None; // more than one argument
        }
        found = Some(child);
    }
    found
}

crate::ast_check! { on ["call_expression"] prefilter = ["import.meta"] => |node, source, ctx, diagnostics|
    if !super::engines_allow_import_meta_dirname(ctx) {
        return;
    }

    // Order matters: the `dirname(...)` and `path.dirname(...)` matches
    // wrap a `fileURLToPath(import.meta.url)` call, so when those match
    // the inner call's diagnostic would be redundant. We dedupe by checking
    // each call's parent: if it's a `dirname` / `path.dirname` wrapper,
    // skip the inner-only diagnostic.

    // 1. `path.dirname(fileURLToPath(import.meta.url))`
    if is_method_call_with_import_meta_url(node, "path", "dirname", source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "prefer-import-meta-properties",
            "Use `import.meta.dirname` instead of `path.dirname(fileURLToPath(import.meta.url))`.".into(),
            Severity::Warning,
        ));
        return;
    }

    // 2. `dirname(fileURLToPath(import.meta.url))`
    if is_call_to_with_import_meta_url_two_levels(node, "dirname", "fileURLToPath", source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "prefer-import-meta-properties",
            "Use `import.meta.dirname` instead of `dirname(fileURLToPath(import.meta.url))`.".into(),
            Severity::Warning,
        ));
        return;
    }

    // 3. `fileURLToPath(import.meta.url)` — but skip if our parent is a
    //    `dirname(...)` or `path.dirname(...)` wrapper already handled above.
    if is_call_to_with_import_meta_url(node, "fileURLToPath", source) {
        if has_dirname_wrapper_parent(node, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "prefer-import-meta-properties",
            "Use `import.meta.filename` instead of `fileURLToPath(import.meta.url)`.".into(),
            Severity::Warning,
        ));
    }
}

/// Match `outer(inner(import.meta.url))` where the callees are bare identifiers.
fn is_call_to_with_import_meta_url_two_levels(
    call: tree_sitter::Node<'_>,
    outer: &str,
    inner: &str,
    source: &[u8],
) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "identifier" {
        return false;
    }
    let outer_text = std::str::from_utf8(&source[func.byte_range()]).unwrap_or("");
    if outer_text != outer {
        return false;
    }
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Some(arg) = single_argument(args, source) else {
        return false;
    };
    arg.kind() == "call_expression" && is_call_to_with_import_meta_url(arg, inner, source)
}

/// Check whether the immediate enclosing call is `dirname(<this>)` or
/// `path.dirname(<this>)` — which would have produced the wrapper-level
/// diagnostic already, so the inner `fileURLToPath` should stay quiet.
fn has_dirname_wrapper_parent(call: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // Walk up: argument → arguments → call_expression
    let Some(args_parent) = call.parent() else {
        return false;
    };
    if args_parent.kind() != "arguments" {
        return false;
    }
    let Some(outer_call) = args_parent.parent() else {
        return false;
    };
    if outer_call.kind() != "call_expression" {
        return false;
    }
    let Some(outer_func) = outer_call.child_by_field_name("function") else {
        return false;
    };
    match outer_func.kind() {
        "identifier" => {
            std::str::from_utf8(&source[outer_func.byte_range()]).ok() == Some("dirname")
        }
        "member_expression" => {
            let obj = outer_func.child_by_field_name("object");
            let prop = outer_func.child_by_field_name("property");
            match (obj, prop) {
                (Some(o), Some(p)) => {
                    let ot = std::str::from_utf8(&source[o.byte_range()]).unwrap_or("");
                    let pt = std::str::from_utf8(&source[p.byte_range()]).unwrap_or("");
                    ot == "path" && pt == "dirname"
                }
                _ => false,
            }
        }
        _ => false,
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_file_url_to_path() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const file = fileURLToPath(import.meta.url);", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.filename"));
    }

    #[test]
    fn flags_dirname_pattern() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const dir = dirname(fileURLToPath(import.meta.url));", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn flags_path_dirname_pattern() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const dir = path.dirname(fileURLToPath(import.meta.url));", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn allows_import_meta_filename() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const file = import.meta.filename;", "t.ts").is_empty());
    }

    #[test]
    fn allows_import_meta_dirname() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const dir = import.meta.dirname;", "t.ts").is_empty());
    }

    #[test]
    fn no_duplicate_for_dirname_containing_file_url() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const dir = dirname(fileURLToPath(import.meta.url));", "t.ts");
        assert_eq!(d.len(), 1);
    }

    /// Run the rule against a file whose nearest `package.json` declares the
    /// given `engines.node` range, so the `engines`-gating path is exercised.
    fn run_with_engine(node_version: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let pkg =
            format!(r#"{{"name":"t","version":"0.0.0","engines":{{"node":"{node_version}"}}}}"#);
        fs::write(dir.path().join("package.json"), pkg).unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile { path: src_path.clone(), language: Language::TypeScript };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &src_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    const DIRNAME_PATTERN: &str = "const __dirname = path.dirname(fileURLToPath(import.meta.url));";

    // Issue #1702: a project supporting Node below 21.2/20.11 must NOT be told
    // to use `import.meta.dirname`, which would break at runtime there.
    #[test]
    fn skips_when_engines_node_below_backport() {
        assert!(run_with_engine(">=12.20.0", DIRNAME_PATTERN).is_empty());
    }

    #[test]
    fn skips_when_engines_node_minimum_is_21_0() {
        // 21.2 is the threshold; 21.0/21.1 lack the properties.
        assert!(run_with_engine(">=21.0.0", DIRNAME_PATTERN).is_empty());
    }

    #[test]
    fn skips_when_engines_node_minimum_is_20_0() {
        // 20.11 is the backport threshold; 20.0–20.10 lack the properties.
        assert!(run_with_engine(">=20.0.0", DIRNAME_PATTERN).is_empty());
    }

    // Negative space: the rule must still fire when the minimum guarantees the
    // properties exist.
    #[test]
    fn fires_when_engines_node_at_21_2() {
        let d = run_with_engine(">=21.2.0", DIRNAME_PATTERN);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn fires_when_engines_node_at_20_11_backport() {
        let d = run_with_engine(">=20.11.0", DIRNAME_PATTERN);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn fires_when_engines_node_well_above_threshold() {
        let d = run_with_engine(">=22.0.0", DIRNAME_PATTERN);
        assert_eq!(d.len(), 1);
    }

    // No `engines.node` constraint: the package targets a modern runtime by
    // default, so the suggestion stands.
    #[test]
    fn fires_when_no_engines_node_constraint() {
        let d = run_without_node_engine(DIRNAME_PATTERN);
        assert_eq!(d.len(), 1);
    }

    fn run_without_node_engine(source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"t","version":"0.0.0"}"#).unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile { path: src_path.clone(), language: Language::TypeScript };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &src_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }
}
