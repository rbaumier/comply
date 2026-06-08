use crate::diagnostic::{Diagnostic, Severity};

/// Strip surrounding quotes/backticks from a string-literal node's text.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

/// Returns true if `arg` is a string literal whose content is `utf-8` / `utf8`,
/// or an object expression containing `encoding: 'utf-8'` / `'utf8'`.
fn is_utf8_encoding_arg(arg: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match arg.kind() {
        "string" => {
            let text = std::str::from_utf8(&source[arg.byte_range()]).unwrap_or("");
            let inner = unquote(text).to_ascii_lowercase();
            inner == "utf-8" || inner == "utf8"
        }
        "object" => {
            // Look for a property named `encoding` whose value is utf-8 / utf8.
            let mut cursor = arg.walk();
            for prop in arg.children(&mut cursor) {
                if prop.kind() != "pair" {
                    continue;
                }
                let Some(key) = prop.child_by_field_name("key") else {
                    continue;
                };
                let Some(value) = prop.child_by_field_name("value") else {
                    continue;
                };
                let key_text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
                if unquote(key_text) != "encoding" {
                    continue;
                }
                if value.kind() == "string" {
                    let val_text = std::str::from_utf8(&source[value.byte_range()]).unwrap_or("");
                    let inner = unquote(val_text).to_ascii_lowercase();
                    if inner == "utf-8" || inner == "utf8" {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Returns true if `call` is a `readFileSync(...)` invocation whose 2nd
/// (or only) argument indicates utf-8 encoding.
fn is_readfilesync_with_utf8(call: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    // Accept both bare `readFileSync(...)` and `fs.readFileSync(...)`.
    let callee_name = match func.kind() {
        "identifier" => std::str::from_utf8(&source[func.byte_range()]).ok(),
        "member_expression" => func
            .child_by_field_name("property")
            .and_then(|p| std::str::from_utf8(&source[p.byte_range()]).ok()),
        _ => None,
    };
    if callee_name != Some("readFileSync") {
        return false;
    }

    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };

    // Collect named (real) arguments; need 2nd one to be the encoding.
    let mut named: Vec<tree_sitter::Node<'_>> = Vec::new();
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.is_named() {
            named.push(child);
        }
    }
    if named.len() < 2 {
        return false;
    }
    is_utf8_encoding_arg(named[1], source)
}

/// Returns true if `call` is `JSON.parse(<single-arg>)`.
fn is_json_parse<'a>(call: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "member_expression" {
        return None;
    }
    let obj = func.child_by_field_name("object")?;
    let prop = func.child_by_field_name("property")?;
    let obj_text = std::str::from_utf8(&source[obj.byte_range()]).ok()?;
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).ok()?;
    if obj_text != "JSON" || prop_text != "parse" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut single = None;
    for child in args.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if single.is_some() {
            return None;
        }
        single = Some(child);
    }
    single
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(arg) = is_json_parse(node, source) else {
        return;
    };
    if arg.kind() != "call_expression" {
        return;
    }
    if !is_readfilesync_with_utf8(arg, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "prefer-json-parse-buffer",
        "Prefer reading the JSON file as a buffer — remove the encoding argument.".into(),
        Severity::Warning,
    ));
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
    fn flags_readfilesync_utf8() {
        let d = crate::rules::test_helpers::run_rule(&Check, r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf-8'));"#, "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-json-parse-buffer");
    }

    #[test]
    fn flags_readfilesync_utf8_no_dash() {
        let d = crate::rules::test_helpers::run_rule(&Check, r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf8'));"#, "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_readfilesync_without_encoding() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"JSON.parse(fs.readFileSync('config.json'))"#, "t.ts").is_empty());
    }

    #[test]
    fn allows_non_utf8_encoding() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"JSON.parse(fs.readFileSync('file', 'ascii'))"#, "t.ts").is_empty());
    }
}
