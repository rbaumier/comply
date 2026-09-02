//! rust-select-in-loop-unpinned backend.
//!
//! Walks `macro_invocation` nodes whose macro name's last path segment is
//! `select` and that sit in a `loop` / `while` / `for` body, then reads the
//! branches out of the macro's `token_tree`.
//!
//! A macro body is an unparsed token stream, so the branches are recovered by
//! scanning the text: [`select_branch_heads`] walks the body one branch at a
//! time (head up to the top-level `=>`, then the handler, block or
//! comma-terminated expression), and [`branch_future`] takes the text right of
//! the first top-level `=` in a head. Both scans skip string literals, char
//! literals and comments, so a `=>` or `,` written inside one is never mistaken
//! for structure.
//!
//! A branch is reported when its future is a direct call —
//! `read_frame(&mut conn)`, `socket.read(&mut buf)` — that is not one of the
//! cancel-safe primitives. Two shapes stay silent:
//!
//! - a binding, with or without `&mut` (`r = &mut fut`): the future was built
//!   and pinned before the loop, which is exactly the fix.
//! - anything the scan cannot pin down as a plain call — an `async` block, a
//!   parenthesised expression, a turbofish. Unrecognised means unreported.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_loop_body, is_in_test_context, trailing_path_segment};

/// Calls that yield a cancel-safe future: dropping one mid-poll loses nothing,
/// so rebuilding it every iteration is free and idiomatic. `timeout` is
/// deliberately absent — dropping it discards the future it wraps.
const CANCEL_SAFE_CALLS: &[&str] = &[
    "recv",
    "next",
    "tick",
    "cancelled",
    "changed",
    "notified",
    "accept",
    "readable",
    "writable",
    "sleep",
];

crate::ast_check! { on ["macro_invocation"] prefilter = ["select!"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }

    let Some(name) = node.child_by_field_name("macro") else { return; };
    if trailing_path_segment(name, source) != Some("select") { return; }
    if !is_in_loop_body(node, source) { return; }
    if is_in_test_context(node, source) { return; }

    let mut cursor = node.walk();
    let Some(body) = node.children(&mut cursor).find(|child| child.kind() == "token_tree") else { return; };
    let Ok(body_text) = body.utf8_text(source) else { return; };

    let Some(branch) = select_branch_heads(strip_delimiters(body_text))
        .into_iter()
        .find(|head| branch_future(head).is_some_and(future_is_rebuilt_here))
    else { return; };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Branch `{}` builds its future inside the loop, so every round another branch wins drops it \
             and loses the progress it had made. Build it once before the loop, `tokio::pin!` it, and select on `&mut` it.",
            collapse_whitespace(branch)
        ),
        Severity::Error,
    ));
}

/// The `pattern = future` head of every branch in a `select!` body, in source
/// order.
///
/// Walks the body branch by branch: everything up to the next top-level `=>` is
/// a head, and what follows is the handler, skipped whole so the `=>` of a
/// nested `match` never registers as a branch of its own. A leading `biased;`
/// or any other statement in front of the first branch is dropped from the head
/// it precedes.
fn select_branch_heads(body: &str) -> Vec<&str> {
    let mut heads = Vec::new();
    let mut index = 0;
    while let Some(arrow) = find_top_level(body, index, Delimiter::Arrow) {
        heads.push(head_without_leading_statements(&body[index..arrow]));
        index = skip_branch_handler(body, arrow + 2);
    }
    heads
}

/// The future half of a branch head — the text right of the first top-level
/// `=`. `else` / `complete` / `default` heads have no `=` and yield `None`, and
/// tokio's `, if <precondition>` suffix is cut off first so it never lands in
/// the future text.
fn branch_future(head: &str) -> Option<&str> {
    let pattern_and_future = match find_top_level(head, 0, Delimiter::Comma) {
        Some(comma) => &head[..comma],
        None => head,
    };
    let equals = find_top_level(pattern_and_future, 0, Delimiter::BindingEquals)?;
    Some(pattern_and_future[equals + 1..].trim())
}

/// True when a branch's future is constructed by the branch itself and is not
/// cancel-safe — the shape that silently discards partial progress once a loop
/// wraps the `select!`.
fn future_is_rebuilt_here(future: &str) -> bool {
    call_name(strip_borrow(future.trim())).is_some_and(|name| !CANCEL_SAFE_CALLS.contains(&name))
}

/// Drop a `&` / `&mut` prefix. `r = &mut fut` polls a future pinned outside the
/// loop, and what is left after the prefix — a bare binding — is what says so.
fn strip_borrow(future: &str) -> &str {
    let borrowed = match future.strip_prefix('&') {
        Some(rest) => rest.trim_start(),
        None => return future,
    };
    match borrowed.strip_prefix("mut ") {
        Some(rest) => rest.trim_start(),
        None => borrowed,
    }
}

/// The name of the call that builds `future`, or `None` when `future` is not a
/// plain call: a binding (`fut`), an `async` block, a parenthesised or
/// otherwise compound expression.
///
/// The callee is read from the *first* top-level `(`, so a wrapped future
/// (`rx.recv().fuse()`) is judged on what it actually awaits, not on the
/// adapter around it.
fn call_name(future: &str) -> Option<&str> {
    if !future.ends_with(')') {
        return None;
    }
    let open = find_top_level(future, 0, Delimiter::OpenParen)?;
    let callee = future[..open].trim_end();
    if callee.is_empty() || !callee.bytes().all(is_path_byte) {
        return None;
    }
    callee.rsplit(['.', ':']).next().filter(|s| !s.is_empty())
}

fn is_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || byte == b':'
}

/// A `biased;` marker, or any other statement, written in front of the branch
/// it precedes: everything up to the last top-level `;` belongs to that
/// statement, not to the branch head.
fn head_without_leading_statements(head: &str) -> &str {
    match find_last_top_level_semicolon(head) {
        Some(semicolon) => head[semicolon + 1..].trim(),
        None => head.trim(),
    }
}

/// The byte offset just past a branch's handler: a `{ … }` block plus an
/// optional trailing `,`, or an expression up to and including the top-level
/// `,` that ends it.
fn skip_branch_handler(body: &str, from: usize) -> usize {
    let handler = skip_trivia(body, from);
    if body.as_bytes().get(handler) == Some(&b'{') {
        let after_block = skip_balanced_group(body, handler);
        let next = skip_trivia(body, after_block);
        return if body.as_bytes().get(next) == Some(&b',') {
            next + 1
        } else {
            after_block
        };
    }
    match find_top_level(body, handler, Delimiter::Comma) {
        Some(comma) => comma + 1,
        None => body.len(),
    }
}

/// What [`find_top_level`] is looking for, all of them only counted at bracket
/// depth zero.
#[derive(Clone, Copy, PartialEq)]
enum Delimiter {
    Arrow,
    Comma,
    OpenParen,
    /// The `=` of a `pattern = future` binding: a single `=` that is not part of
    /// a comparison (`==`, `!=`, `<=`, `>=`), of the branch arrow (`=>`), or of
    /// an inclusive range pattern (`..=`).
    BindingEquals,
}

/// The offset of the first `delimiter` at bracket depth zero, searching from
/// `from`. Returns `None` at end of input or when the enclosing group closes
/// first, so a scan started inside a group never escapes it.
fn find_top_level(text: &str, from: usize, delimiter: Delimiter) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = from;
    while index < bytes.len() {
        if let Some(after) = skip_opaque(text, index) {
            index = after;
            continue;
        }
        match bytes[index] {
            b'(' if depth == 0 && delimiter == Delimiter::OpenParen => return Some(index),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b',' if depth == 0 && delimiter == Delimiter::Comma => return Some(index),
            b'=' if depth == 0 && bytes.get(index + 1) == Some(&b'>') => {
                if delimiter == Delimiter::Arrow {
                    return Some(index);
                }
                index += 1;
            }
            b'=' if depth == 0
                && delimiter == Delimiter::BindingEquals
                && is_binding_equals(bytes, index) =>
            {
                return Some(index);
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// True when the `=` at `index` binds a pattern to a future rather than being
/// one half of a two-character operator.
fn is_binding_equals(bytes: &[u8], index: usize) -> bool {
    let follows = index
        .checked_sub(1)
        .is_none_or(|before| !matches!(bytes[before], b'=' | b'!' | b'<' | b'>' | b'.'));
    follows && bytes.get(index + 1) != Some(&b'=')
}

/// The last top-level `;` in `text`, or `None` when it has none.
fn find_last_top_level_semicolon(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = 0;
    let mut last = None;
    while index < bytes.len() {
        if let Some(after) = skip_opaque(text, index) {
            index = after;
            continue;
        }
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => last = Some(index),
            _ => {}
        }
        index += 1;
    }
    last
}

/// The offset just past the bracket group that opens at `open`.
fn skip_balanced_group(text: &str, open: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        if let Some(after) = skip_opaque(text, index) {
            index = after;
            continue;
        }
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    bytes.len()
}

/// The offset of the next byte that is neither whitespace nor part of a
/// comment.
fn skip_trivia(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        match skip_comment(text, index) {
            Some(after) => index = after,
            None => return index,
        }
    }
    index
}

/// If `text[index..]` opens a run of bytes that must not be read as structure —
/// a string literal, a raw string, a char literal, a comment — return the offset
/// just past it. `None` when the byte at `index` is ordinary code.
fn skip_opaque(text: &str, index: usize) -> Option<usize> {
    skip_comment(text, index)
        .or_else(|| skip_raw_string(text, index))
        .or_else(|| skip_string(text, index))
        .or_else(|| skip_char_literal(text, index))
}

fn skip_comment(text: &str, index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(index) != Some(&b'/') {
        return None;
    }
    match bytes.get(index + 1) {
        Some(b'/') => Some(
            text[index..]
                .find('\n')
                .map_or(bytes.len(), |offset| index + offset + 1),
        ),
        Some(b'*') => Some(
            text[index + 2..]
                .find("*/")
                .map_or(bytes.len(), |offset| index + 2 + offset + 2),
        ),
        _ => None,
    }
}

fn skip_string(text: &str, index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    let mut scan = index + 1;
    while scan < bytes.len() {
        match bytes[scan] {
            // An escaped byte can be a `"` or a `\`; either way it is consumed
            // whole so it cannot close the literal.
            b'\\' => scan += 2,
            b'"' => return Some(scan + 1),
            _ => scan += 1,
        }
    }
    Some(bytes.len())
}

/// A raw string (`r"…"`, `r#"…"#`): no escapes, and the closing quote must be
/// followed by as many `#` as the opening one had.
fn skip_raw_string(text: &str, index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    // A leading `r` is only a raw-string prefix when it does not continue an
    // identifier — otherwise `str"` inside `let s: str` would start one.
    if index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_') {
        return None;
    }
    let hashes = bytes[index + 1..]
        .iter()
        .take_while(|byte| **byte == b'#')
        .count();
    if bytes.get(index + 1 + hashes) != Some(&b'"') {
        return None;
    }
    let terminator = format!("\"{}", "#".repeat(hashes));
    let body_start = index + 2 + hashes;
    Some(
        text.get(body_start..)
            .and_then(|rest| rest.find(&terminator))
            .map_or(bytes.len(), |offset| body_start + offset + terminator.len()),
    )
}

/// A char literal (`'x'`, `'\n'`). A lifetime (`&'a mut T`, `'static`) opens
/// with the same quote and is left alone: it has no closing quote.
fn skip_char_literal(text: &str, index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(index) != Some(&b'\'') {
        return None;
    }
    if bytes.get(index + 1) == Some(&b'\\') {
        return text[index + 2..]
            .find('\'')
            .map(|offset| index + 2 + offset + 1);
    }
    let content = text.get(index + 1..)?.chars().next()?;
    let close = index + 1 + content.len_utf8();
    (bytes.get(close) == Some(&b'\'')).then_some(close + 1)
}

/// Drop the delimiters a macro body is wrapped in, so the scan starts on the
/// first branch.
fn strip_delimiters(token_tree: &str) -> &str {
    let trimmed = token_tree.trim();
    let mut chars = trimmed.chars();
    match (chars.next(), chars.next_back()) {
        (Some('{' | '(' | '['), Some('}' | ')' | ']')) => chars.as_str(),
        _ => trimmed,
    }
}

/// Squeeze a branch head onto one line so a multi-line branch still reads as a
/// single quoted fragment in the diagnostic.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    use super::{Check, branch_future, select_branch_heads, strip_delimiters};
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    fn heads(body: &str) -> Vec<&str> {
        select_branch_heads(strip_delimiters(body))
    }

    #[test]
    fn splits_block_bodied_branches() {
        let body = "{ a = f() => { g(); } b = h() => { i(); } }";
        assert_eq!(heads(body), vec!["a = f()", "b = h()"]);
    }

    #[test]
    fn splits_expression_bodied_branches() {
        let body = "{ a = f() => break, b = h() => continue, }";
        assert_eq!(heads(body), vec!["a = f()", "b = h()"]);
    }

    #[test]
    fn ignores_arrow_inside_a_handler() {
        let body = "{ a = f() => { match x { 1 => 2, _ => 3 } } b = h() => {} }";
        assert_eq!(heads(body), vec!["a = f()", "b = h()"]);
    }

    #[test]
    fn ignores_arrow_inside_a_string() {
        let body = r#"{ a = f() => { log("x => y"); } b = h() => {} }"#;
        assert_eq!(heads(body), vec!["a = f()", "b = h()"]);
    }

    #[test]
    fn drops_the_biased_marker_from_the_first_head() {
        let body = "{ biased; a = f() => {} b = h() => {} }";
        assert_eq!(heads(body), vec!["a = f()", "b = h()"]);
    }

    #[test]
    fn branch_future_reads_the_right_of_the_binding() {
        assert_eq!(branch_future("Some(v) = rx.recv()"), Some("rx.recv()"));
        assert_eq!(branch_future("r = &mut fut"), Some("&mut fut"));
    }

    #[test]
    fn branch_future_drops_a_tokio_precondition() {
        assert_eq!(branch_future("v = f(), if ready"), Some("f()"));
    }

    #[test]
    fn branch_future_skips_keyword_arms() {
        assert_eq!(branch_future("else"), None);
        assert_eq!(branch_future("complete"), None);
    }

    #[test]
    fn flags_free_function_future_rebuilt_each_iteration() {
        let src = r#"
async fn run(conn: &mut Conn, rx: &mut Receiver<u32>) {
    loop {
        tokio::select! {
            frame = read_frame(&mut conn) => { handle(frame); }
            Some(msg) = rx.recv() => { send(msg); }
        }
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_method_future_rebuilt_each_iteration() {
        let src = r#"
async fn run(socket: &mut TcpStream, buf: &mut [u8], token: &CancellationToken) {
    while running() {
        tokio::select! {
            n = socket.read(&mut buf) => { use_bytes(n); }
            _ = token.cancelled() => break,
        }
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_timeout_wrapped_future() {
        let src = r#"
async fn run(conn: &mut Conn, rx: &mut Receiver<u32>) {
    loop {
        tokio::select! {
            r = tokio::time::timeout(dur, read_frame(&mut conn)) => { handle(r); }
            Some(m) = rx.recv() => { send(m); }
        }
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_futures_select_too() {
        let src = r#"
async fn run(conn: &mut Conn, ticker: &mut Interval) {
    loop {
        futures::select! {
            frame = read_frame(&mut conn) => handle(frame),
            _ = ticker.tick() => refresh(),
        }
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multi_line_branch_head_in_a_for_loop() {
        let src = r#"
async fn run(conns: Vec<Conn>, rx: &mut Receiver<u32>) {
    for mut conn in conns {
        tokio::select! {
            frame = read_frame(
                &mut conn,
            ) => { handle(frame); }
            Some(msg) = rx.recv() => { send(msg); }
        }
    }
}
"#;
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("`frame = read_frame( &mut conn, )`")
        );
    }

    #[test]
    fn allows_pinned_future_polled_by_mut_reference() {
        let src = r#"
async fn run(conn: &mut Conn, rx: &mut Receiver<u32>) {
    let fut = read_frame(&mut conn);
    tokio::pin!(fut);
    loop {
        tokio::select! {
            frame = &mut fut => { handle(frame); }
            Some(msg) = rx.recv() => { send(msg); }
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_only_cancel_safe_branches() {
        let src = r#"
async fn run(rx: &mut Receiver<u32>, ticker: &mut Interval, token: &CancellationToken) {
    loop {
        tokio::select! {
            Some(msg) = rx.recv() => { send(msg); }
            _ = ticker.tick() => { refresh(); }
            _ = token.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_secs(1)) => { poke(); }
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_select_outside_a_loop() {
        let src = r#"
async fn run(conn: &mut Conn, rx: &mut Receiver<u32>) {
    tokio::select! {
        frame = read_frame(&mut conn) => { handle(frame); }
        Some(msg) = rx.recv() => { send(msg); }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rebuilt_future_in_a_test() {
        let src = r#"
#[tokio::test]
async fn t() {
    loop {
        tokio::select! {
            frame = read_frame(&mut conn) => { handle(frame); }
            Some(msg) = rx.recv() => { send(msg); }
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_macro_in_loop() {
        let src = r#"
fn run(q: &Query) {
    loop {
        my_dsl::pick! {
            frame = read_frame(&mut conn) => { handle(frame); }
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_async_block_branch() {
        let src = r#"
async fn run(rx: &mut Receiver<u32>) {
    loop {
        tokio::select! {
            v = async { compute().await } => { use_it(v); }
            Some(msg) = rx.recv() => { send(msg); }
        }
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn reports_one_diagnostic_naming_the_branch() {
        let src = r#"
async fn run(conn: &mut Conn, other: &mut Conn) {
    loop {
        tokio::select! {
            a = read_frame(&mut conn) => { handle(a); }
            b = read_frame(&mut other) => { handle(b); }
        }
    }
}
"#;
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("`a = read_frame(&mut conn)`"));
    }
}
