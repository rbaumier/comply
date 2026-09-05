//! rust-unbuffered-file-io-in-loop backend.
//!
//! Anchored on `let_declaration`. A declaration qualifies when its initializer
//! is a raw `File` constructor — `File::open(..)` / `File::create(..)` under any
//! module path (`std::fs::`, `fs::`, `tokio::fs::`), or an `OpenOptions` chain
//! ending in `.open(..)` — optionally followed by `?`, `.await`, `.unwrap()` or
//! `.expect(..)`. An initializer that already hands the file to a buffer
//! (`BufReader::new(File::open(p)?)`) is not a raw `File` and never matches.
//!
//! The enclosing function body is then scanned for uses of the bound name:
//!
//! - a per-call `io` method (`read`, `read_exact`, `read_line`, `read_to_string`,
//!   `read_to_end`, `write`, `write_all`, `write_fmt`) or a `write!`/`writeln!`
//!   macro targeting the handle, sitting inside a loop body, is the violation;
//! - the handle being passed to `BufReader`/`BufWriter`/`LineWriter`
//!   (`::new` or `::with_capacity`, through `&` / `&mut`), passed to a `copy`
//!   free function (`io::copy` drives its own buffer), or locked
//!   (`file.lock()`, whose point is the file descriptor, not the byte stream)
//!   exempts the declaration wherever it appears in the function.
//!
//! A handle used only for `metadata` / `set_len` / `sync_all` / `seek`, or read
//! whole once outside any loop (`read_to_string`, `read_to_end`), never reaches
//! the loop condition and is not flagged. Test code is exempt: a fixture that
//! writes a handful of lines is not a performance problem.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{enclosing_fn, is_in_loop_body, is_test_code};

/// `io::Read` / `io::Write` methods that issue one syscall per call on an
/// unbuffered handle. `read_to_string` / `read_to_end` read the whole file in
/// one call, which is fine once — the rule only sees them inside a loop, where
/// re-reading the file every iteration is the problem.
const PER_CALL_IO_METHODS: &[&str] = &[
    "read",
    "read_exact",
    "read_line",
    "read_to_string",
    "read_to_end",
    "write",
    "write_all",
    "write_fmt",
];

/// Macros that expand to `write_fmt` on their first argument.
const WRITE_MACROS: &[&str] = &["write", "writeln"];

/// Adapters that put a buffer in front of the handle. Matched together with a
/// `new` / `with_capacity` constructor segment.
const BUFFERED_WRAPPERS: &[&str] = &["BufReader", "BufWriter", "LineWriter"];

/// Constructor segments of the buffered adapters above.
const WRAPPER_CONSTRUCTORS: &[&str] = &["new", "with_capacity"];

/// Postfix method calls that only unwrap the `io::Result` around a constructor.
const RESULT_UNWRAP_METHODS: &[&str] = &["unwrap", "expect"];

crate::ast_check! {
    on ["let_declaration"]
    prefilter = ["File::open", "File::create", "OpenOptions"]
    => |node, source, ctx, diagnostics|

    if ctx.file.path_segments.in_test_dir { return; }
    if is_test_code(node, source, ctx) { return; }

    let Some(name) = let_binding_name(node, source) else { return; };
    let Some(value) = node.child_by_field_name("value") else { return; };
    if !opens_raw_file(value, source) { return; }

    // Uses of the handle are scanned across the whole enclosing function: the
    // buffering fix can be applied at the binding no matter where the loop or
    // the later re-wrap sits.
    let Some(body) = enclosing_fn(node).and_then(|f| f.child_by_field_name("body")) else {
        return;
    };
    let mut usage = Usage::default();
    scan_handle_uses(body, source, name, &mut usage);
    if usage.exempted || !usage.io_in_loop { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{name}` is an unbuffered `File` read or written inside a loop — one syscall per iteration. \
             Bind it as `BufReader::new(File::open(..)?)` (or `BufWriter::new(File::create(..)?)`, flushed before it drops)."
        ),
        Severity::Error,
    ));
}

/// What the enclosing function does with the bound handle.
#[derive(Default)]
struct Usage {
    /// The handle is buffered later, handed to `io::copy`, or locked — the
    /// per-iteration syscall is either already amortised or beside the point.
    exempted: bool,
    /// A per-call `io` method or `write!`/`writeln!` targets the handle from
    /// inside a loop body.
    io_in_loop: bool,
}

/// The identifier a `let` binds, for the two forms that give the handle a
/// usable name. A destructuring or `_` pattern has no single name to track, so
/// the rule stays out of it.
fn let_binding_name<'a>(let_decl: Node, source: &'a [u8]) -> Option<&'a str> {
    let pattern = let_decl.child_by_field_name("pattern")?;
    let identifier = match pattern.kind() {
        "identifier" => pattern,
        // `let mut f = …` wraps the name in a `mut_pattern`.
        "mut_pattern" => pattern.named_child(0).filter(|n| n.kind() == "identifier")?,
        _ => return None,
    };
    identifier.utf8_text(source).ok()
}

/// True when the initializer produces a `File` with no buffer in front of it.
fn opens_raw_file(value: Node, source: &[u8]) -> bool {
    let core = strip_result_postfix(value, source);
    if core.kind() != "call_expression" {
        return false;
    }
    let Some(function) = core.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = function.utf8_text(source) else {
        return false;
    };
    if is_file_constructor_path(text) {
        return true;
    }
    // `OpenOptions::new().append(true).open(path)` — a method call whose
    // receiver chain names `OpenOptions`, so `dir.open(name)` does not match.
    function.kind() == "field_expression"
        && field_name(function, source) == Some("open")
        && text.contains("OpenOptions")
}

/// Peel `?`, `.await` and the `unwrap`/`expect` postfix calls that only remove
/// the `io::Result`, leaving the constructor call itself.
fn strip_result_postfix<'tree>(node: Node<'tree>, source: &[u8]) -> Node<'tree> {
    let mut current = node;
    loop {
        match current.kind() {
            "try_expression" | "await_expression" => {
                let Some(inner) = current.named_child(0) else {
                    return current;
                };
                current = inner;
            }
            "call_expression" => {
                let Some(function) = current.child_by_field_name("function") else {
                    return current;
                };
                if function.kind() != "field_expression" {
                    return current;
                }
                let Some(method) = field_name(function, source) else {
                    return current;
                };
                if !RESULT_UNWRAP_METHODS.contains(&method) {
                    return current;
                }
                let Some(receiver) = function.child_by_field_name("value") else {
                    return current;
                };
                current = receiver;
            }
            _ => return current,
        }
    }
}

/// True for a path whose last two segments are `File::open` or `File::create`,
/// so `std::fs::File::open` and `tokio::fs::File::create` match while a
/// same-named method on another type does not.
fn is_file_constructor_path(text: &str) -> bool {
    let mut segments = text.rsplit("::");
    let (Some(last), Some(owner)) = (segments.next(), segments.next()) else {
        return false;
    };
    owner == "File" && (last == "open" || last == "create")
}

/// Recursively classify every use of `name` in the subtree. Both findings are
/// accumulated rather than short-circuited: an exemption anywhere in the
/// function outranks a loop use found earlier.
fn scan_handle_uses(node: Node, source: &[u8], name: &str, usage: &mut Usage) {
    match node.kind() {
        "call_expression" => classify_call(node, source, name, usage),
        "macro_invocation" => classify_write_macro(node, source, name, usage),
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        scan_handle_uses(child, source, name, usage);
    }
}

fn classify_call(call: Node, source: &[u8], name: &str, usage: &mut Usage) {
    let Some(function) = call.child_by_field_name("function") else {
        return;
    };

    if function.kind() == "field_expression" {
        // `f.read_exact(..)`, `f.lock()` — only a call directly on the handle
        // counts; `f.by_ref().read(..)` reads through an adapter we don't model.
        let Some(receiver) = function.child_by_field_name("value") else {
            return;
        };
        if receiver.kind() != "identifier" || receiver.utf8_text(source) != Ok(name) {
            return;
        }
        let Some(method) = field_name(function, source) else {
            return;
        };
        // `file.lock()` takes an advisory lock on the descriptor; the handle is
        // there for the lock, not for a byte stream worth buffering.
        if method == "lock" {
            usage.exempted = true;
        } else if PER_CALL_IO_METHODS.contains(&method) && is_in_loop_body(call, source) {
            usage.io_in_loop = true;
        }
        return;
    }

    let Ok(text) = function.utf8_text(source) else {
        return;
    };
    // `io::copy(&mut f, &mut out)` runs its own buffered loop, and
    // `BufReader::new(f)` buffers the handle after the fact — both make the
    // binding correct as written.
    if (is_buffered_wrapper_path(text) || path_tail(text) == "copy")
        && call_mentions_identifier(call, source, name)
    {
        usage.exempted = true;
    }
}

/// `write!(f, …)` / `writeln!(f, …)` expand to `f.write_fmt(..)`. The target is
/// the first identifier token of the macro's token tree, which also sees
/// through `write!(&mut f, …)`.
fn classify_write_macro(invocation: Node, source: &[u8], name: &str, usage: &mut Usage) {
    let Some(macro_name) = invocation
        .child_by_field_name("macro")
        .and_then(|m| m.utf8_text(source).ok())
    else {
        return;
    };
    if !WRITE_MACROS.contains(&macro_name) {
        return;
    }
    let mut invocation_cursor = invocation.walk();
    let Some(tokens) = invocation
        .named_children(&mut invocation_cursor)
        .find(|c| c.kind() == "token_tree")
    else {
        return;
    };
    let mut token_cursor = tokens.walk();
    let targets_handle = tokens
        .named_children(&mut token_cursor)
        .find(|c| c.kind() == "identifier")
        .and_then(|c| c.utf8_text(source).ok())
        == Some(name);
    if targets_handle && is_in_loop_body(invocation, source) {
        usage.io_in_loop = true;
    }
}

/// True for a path like `BufReader::new` / `io::BufWriter::with_capacity`.
fn is_buffered_wrapper_path(text: &str) -> bool {
    let mut segments = text.rsplit("::");
    let (Some(constructor), Some(adapter)) = (segments.next(), segments.next()) else {
        return false;
    };
    WRAPPER_CONSTRUCTORS.contains(&constructor) && BUFFERED_WRAPPERS.contains(&adapter)
}

fn path_tail(text: &str) -> &str {
    text.rsplit("::").next().unwrap_or(text)
}

fn field_name<'a>(field_expression: Node, source: &'a [u8]) -> Option<&'a str> {
    field_expression
        .child_by_field_name("field")?
        .utf8_text(source)
        .ok()
}

/// True when `name` appears anywhere in the call's argument list — deep enough
/// to see through `&mut f` and through a leading capacity argument.
fn call_mentions_identifier(call: Node, source: &[u8], name: &str) -> bool {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    subtree_has_identifier(arguments, source, name)
}

fn subtree_has_identifier(node: Node, source: &[u8], name: &str) -> bool {
    if node.kind() == "identifier" && node.utf8_text(source) == Ok(name) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| subtree_has_identifier(child, source, name))
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_read_exact_in_while_loop() {
        let src = r#"
fn f(path: &str) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    while more() {
        file.read_exact(&mut buf)?;
    }
    Ok(())
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_write_all_in_for_loop() {
        let src = r#"
fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut out = std::fs::File::create(path)?;
    for row in rows {
        out.write_all(row.as_bytes())?;
    }
    Ok(())
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_writeln_macro_in_loop() {
        let src = r#"
fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut out = File::create(path)?;
    for row in rows {
        writeln!(out, "{row}")?;
    }
    Ok(())
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_open_options_handle_written_in_loop() {
        let src = r#"
fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut log = OpenOptions::new().append(true).open(path)?;
    for row in rows {
        log.write_all(row.as_bytes())?;
    }
    Ok(())
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unwrapped_handle_read_in_loop() {
        let src = r#"
fn f(path: &str) {
    let mut file = File::open(path).unwrap();
    loop {
        file.read(&mut buf).unwrap();
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_tokio_file_written_in_loop() {
        let src = r#"
async fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut out = tokio::fs::File::create(path).await?;
    for row in rows {
        out.write_all(row.as_bytes()).await?;
    }
    Ok(())
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_handle_written_in_for_each_closure() {
        let src = r#"
fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut out = File::create(path)?;
    rows.iter().for_each(|row| {
        out.write_all(row.as_bytes()).unwrap();
    });
    Ok(())
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_read_to_string_outside_loop() {
        let src = r#"
fn f(path: &str) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_handle_buffered_at_the_binding() {
        let src = r#"
fn f(path: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    for line in 0..10 {
        reader.read_line(&mut buf)?;
    }
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_handle_buffered_later_in_the_function() {
        let src = r#"
fn f(path: &str) -> std::io::Result<()> {
    let file = File::open(path)?;
    let mut reader = std::io::BufReader::new(&file);
    for line in 0..10 {
        file.read_line(&mut buf)?;
    }
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_handle_handed_to_io_copy() {
        let src = r#"
fn f(path: &str, out: &mut Vec<u8>) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    for _ in 0..3 {
        file.read(out)?;
    }
    std::io::copy(&mut file, out)?;
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_handle_used_only_for_metadata_in_loop() {
        let src = r#"
fn f(path: &str) -> std::io::Result<()> {
    let file = File::open(path)?;
    loop {
        let len = file.metadata()?.len();
        file.sync_all()?;
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_locked_handle_written_in_loop() {
        let src = r#"
fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.lock()?;
    for row in rows {
        file.write_all(row.as_bytes())?;
    }
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_write_in_loop_on_a_different_handle() {
        let src = r#"
fn f(path: &str, rows: &[Row], sink: &mut Vec<u8>) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for row in rows {
        sink.write_all(row.as_bytes())?;
    }
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_loop_write_through_an_inner_closure_boundary() {
        let src = r#"
fn f(path: &str, rows: &[Row]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for row in rows {
        register(move || { file.write_all(row.as_bytes()).unwrap(); });
    }
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = r#"
#[cfg(test)]
mod tests {
    fn f(path: &str, rows: &[Row]) {
        let mut out = File::create(path).unwrap();
        for row in rows {
            out.write_all(row.as_bytes()).unwrap();
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_write_macro_targeting_another_writer_in_loop() {
        let src = r#"
fn f(path: &str, rows: &[Row], sink: &mut String) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for row in rows {
        write!(sink, "{row}")?;
    }
    Ok(())
}
"#;
        assert!(run(src).is_empty());
    }
}
