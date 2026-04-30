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
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_file_url_to_path() {
        let d = run_ts("const file = fileURLToPath(import.meta.url);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.filename"));
    }

    #[test]
    fn flags_dirname_pattern() {
        let d = run_ts("const dir = dirname(fileURLToPath(import.meta.url));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn flags_path_dirname_pattern() {
        let d = run_ts("const dir = path.dirname(fileURLToPath(import.meta.url));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn allows_import_meta_filename() {
        assert!(run_ts("const file = import.meta.filename;").is_empty());
    }

    #[test]
    fn allows_import_meta_dirname() {
        assert!(run_ts("const dir = import.meta.dirname;").is_empty());
    }

    #[test]
    fn no_duplicate_for_dirname_containing_file_url() {
        let d = run_ts("const dir = dirname(fileURLToPath(import.meta.url));");
        assert_eq!(d.len(), 1);
    }
}
