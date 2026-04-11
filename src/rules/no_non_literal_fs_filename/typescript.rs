use crate::diagnostic::{Diagnostic, Severity};

/// Common fs methods whose first argument should be a string literal.
const FS_METHODS: &[&str] = &[
    "readFile", "readFileSync", "writeFile", "writeFileSync",
    "readdir", "readdirSync", "unlink", "unlinkSync",
    "rmdir", "rmdirSync", "mkdir", "mkdirSync",
    "stat", "statSync", "lstat", "lstatSync",
    "rename", "renameSync", "copyFile", "copyFileSync",
    "access", "accessSync", "appendFile", "appendFileSync",
    "chmod", "chmodSync", "chown", "chownSync",
    "createReadStream", "createWriteStream",
    "open", "openSync", "realpath", "realpathSync",
    "rm", "rmSync",
];

/// Check if the first argument node looks like a literal or safe expression.
fn is_literal_arg(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "string" | "template_string" => true,
        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("");
            text == "__dirname" || text == "__filename"
        }
        "call_expression" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.starts_with("path.join(")
                || text.starts_with("path.resolve(")
                || text.starts_with("new URL(")
        }
        "new_expression" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.starts_with("new URL(")
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    let obj_text = obj.utf8_text(source).unwrap_or("");
    if obj_text != "fs" && obj_text != "fsPromises" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !FS_METHODS.contains(&method) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // First named child of the argument list is the first argument.
    let Some(first_arg) = args.named_child(0) else { return };
    if is_literal_arg(first_arg, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-non-literal-fs-filename".into(),
        message: "Non-literal filename in `fs` call — use a string literal or validate the path.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_variable_filename() {
        assert_eq!(run("fs.readFile(userInput, cb);").len(), 1);
    }

    #[test]
    fn flags_variable_in_write() {
        assert_eq!(run("fs.writeFileSync(filePath, data);").len(), 1);
    }

    #[test]
    fn allows_literal_filename() {
        assert!(run(r#"fs.readFile("config.json", cb);"#).is_empty());
    }

    #[test]
    fn allows_path_join() {
        assert!(run("fs.readFile(path.join(__dirname, 'file.txt'), cb);").is_empty());
    }

    #[test]
    fn allows_no_fs_call() {
        assert!(run("const x = 42;").is_empty());
    }
}
