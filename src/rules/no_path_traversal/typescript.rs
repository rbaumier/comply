//! no-path-traversal backend — flag `fs.readFile(...)` / `fs.writeFile(...)`
//! / etc. calls whose path argument references user-controlled data
//! (`req.params`, `req.query`, `req.body`, `searchParams.get(...)`)
//! and is not first run through a sanitizer (`path.basename`, `path.resolve`).

use crate::diagnostic::{Diagnostic, Severity};

const FS_METHODS: &[&str] = &[
    "readFile",
    "readFileSync",
    "writeFile",
    "writeFileSync",
    "unlink",
    "unlinkSync",
    "createReadStream",
    "createWriteStream",
    "appendFile",
    "appendFileSync",
];

const USER_DATA_NEEDLES: &[&str] = &[
    "req.params",
    "req.query",
    "req.body",
    "request.params",
    "request.query",
    "request.body",
    "searchParams.get",
    "params.",
];

const SANITIZER_NEEDLES: &[&str] = &["basename(", "path.resolve(", "normalize("];

fn is_fs_call(name: &str) -> bool {
    let tail = name.rsplit('.').next().unwrap_or(name);
    FS_METHODS.contains(&tail)
}

fn arg_is_user_controlled(text: &str) -> bool {
    if SANITIZER_NEEDLES.iter().any(|s| text.contains(s)) {
        return false;
    }
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_fs_call(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first = args.named_children(&mut cursor).next();
    let Some(first) = first else { return };
    let Ok(text) = first.utf8_text(source) else { return };
    if arg_is_user_controlled(text) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-path-traversal",
            "User-controlled path in `fs` call — use `path.basename()` or validate against a safe root.".into(),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_fs_read_with_req_params() {
        assert_eq!(run_on("fs.readFile(req.params.filename)").len(), 1);
    }

    #[test]
    fn flags_write_with_query() {
        assert_eq!(run_on("fs.writeFile(req.query.path, data)").len(), 1);
    }

    #[test]
    fn allows_basename_sanitization() {
        assert!(run_on("fs.readFile(path.basename(req.params.filename))").is_empty());
    }

    #[test]
    fn allows_literal_path() {
        assert!(run_on("fs.readFile('/data/file.txt')").is_empty());
    }
}
