//! Detection: a `function_item` whose body references `std::fs::*`
//! (or the common re-exported form `fs::read`, `File::open`, …) and
//! whose parameter list contains a path-shaped parameter.
//!
//! A `&Path` parameter is always path-shaped. A `&str` parameter is
//! flagged only when it is actually passed to one of the body's fs
//! calls — otherwise it may be unrelated text (a title, label, …) that
//! merely shares a function with some fs I/O.
//!
//! We heuristically flag only the first offending parameter per
//! function so the rule doesn't spam on helpers that take two paths.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_effectively_pub;

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return; };
    let Ok(body_text) = body.utf8_text(source) else { return; };
    if !body_uses_fs(body_text) { return; }
    if !is_effectively_pub(node, source, ctx.path) { return; }

    let Some(params) = node.child_by_field_name("parameters") else { return; };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        if param.kind() != "parameter" { continue; }
        let Some(type_node) = param.child_by_field_name("type") else { continue; };
        let Some(label) = classify_path_like_type(type_node, source) else { continue; };

        // A `&str` parameter is only path-shaped when it is actually fed to one
        // of the fs calls in the body — otherwise it could be a title, label or
        // any other text (#4716). A `&Path` parameter is unambiguously a path,
        // so it stays flagged regardless of how the body uses it.
        if label == "&str" {
            let Some(name) = param
                .child_by_field_name("pattern")
                .and_then(|pat| param_name(pat, source))
            else { continue; };
            if !param_passed_to_fs_call(body, name, source) { continue; }
        }

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &type_node,
            super::META.id,
            format!(
                "filesystem function takes `{label}` — use `impl AsRef<Path>` \
                 so callers can pass `&str`, `String`, `&Path`, or `PathBuf` \
                 without converting."
            ),
            Severity::Warning,
        ));
        // Report once per function to avoid noise on two-path helpers.
        break;
    }
}

/// Markers that identify a filesystem call by its callee text. A function whose
/// body contains any of these is doing fs I/O.
const FS_CALL_MARKERS: &[&str] = &[
    "fs::read",
    "fs::write",
    "fs::remove",
    "fs::create_dir",
    "fs::metadata",
    "fs::copy",
    "fs::rename",
    "fs::File",
    "File::open",
    "File::create",
    "OpenOptions::",
];

fn body_uses_fs(body: &str) -> bool {
    // Cheap substring scan — tree-sitter-level precision isn't worth
    // the traversal cost for a best-effort heuristic.
    FS_CALL_MARKERS.iter().any(|marker| body.contains(marker))
}

/// The bare identifier of a parameter pattern, unwrapping a leading `mut`
/// (`mut name: &str` → `name`). Returns `None` for non-binding patterns we
/// can't match by name (tuple/struct destructuring), which conservatively
/// keeps the `&str` param unflagged.
fn param_name<'a>(pattern: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match pattern.kind() {
        "identifier" => pattern.utf8_text(source).ok(),
        "mut_pattern" => {
            let mut cursor = pattern.walk();
            pattern
                .named_children(&mut cursor)
                .find(|child| child.kind() == "identifier")
                .and_then(|child| child.utf8_text(source).ok())
        }
        _ => None,
    }
}

/// True if the parameter named `name` is passed as a *direct* path argument to
/// one of the body's filesystem calls. Walks every `call_expression` whose
/// callee matches an [`FS_CALL_MARKERS`] entry and checks its argument list via
/// [`param_is_direct_path_arg`] (covering `f(name)`, `f(&name)`,
/// `f(name.as_ref())`, but not `f(build_path(name))` where `name` is only an
/// argument to an inner path-builder).
///
/// The match is name-based, not scope-accurate: it does not follow rebindings
/// (`let p = name; fs::read(p)` is missed) nor distinguish a shadowing local of
/// the same name. That is an acceptable trade for a best-effort lint — it cuts
/// the title-string false positive (#4716) without resolving bindings.
fn param_passed_to_fs_call(body: Node, name: &str, source: &[u8]) -> bool {
    let mut found = false;
    walk_call_expressions(body, &mut |call| {
        if found {
            return;
        }
        let Some(func) = call.child_by_field_name("function") else { return; };
        let Ok(func_text) = func.utf8_text(source) else { return; };
        if !callee_is_fs(func_text) {
            return;
        }
        let Some(args) = call.child_by_field_name("arguments") else { return; };
        if param_is_direct_path_arg(args, name, source) {
            found = true;
        }
    });
    found
}

/// True if `callee` (a call expression's function text) is one of the fs calls.
/// A marker must sit on a path-segment boundary so a user callee like
/// `XFile::open` or `prefs::read` does not match `File::open` / `fs::read`.
fn callee_is_fs(callee: &str) -> bool {
    FS_CALL_MARKERS.iter().any(|marker| {
        callee.match_indices(marker).any(|(idx, _)| {
            idx == 0
                || !callee[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    })
}

/// Depth-first walk invoking `visit` for every `call_expression` descendant of
/// `node` (including `node` itself if it is one).
fn walk_call_expressions(node: Node, visit: &mut impl FnMut(Node)) {
    if node.kind() == "call_expression" {
        visit(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_call_expressions(child, visit);
    }
}

/// True if the param `name` occupies a *direct* path position under `node`
/// (an fs call's argument list): the bare identifier (`f(name)`), under a
/// reference (`f(&name)`), or as the receiver of a method chain on the param
/// itself (`f(name.as_ref())` / `f(name.as_path())`).
///
/// A nested `call_expression` transforms its arguments — its *return*, not the
/// arguments, is what reaches the fs call — so we follow only the callee
/// (`function` field) and never descend the `arguments` field. That reaches a
/// method-chain receiver like `name.as_ref()` while leaving a param passed
/// *into* an inner path-builder (`fs::metadata(build_path(name))`,
/// `fs::read(dir.join(name))`) unmatched.
fn param_is_direct_path_arg(node: Node, name: &str, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" => node.utf8_text(source) == Ok(name),
        "call_expression" => node
            .child_by_field_name("function")
            .is_some_and(|func| param_is_direct_path_arg(func, name, source)),
        _ => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .any(|child| param_is_direct_path_arg(child, name, source))
        }
    }
}

/// Classify `type_node` as one of the concrete path-ish types we
/// want to discourage. Returns the human-readable label to cite in
/// the diagnostic, or `None` if the type is fine.
fn classify_path_like_type<'a>(type_node: Node<'a>, source: &'a [u8]) -> Option<&'static str> {
    match type_node.kind() {
        "reference_type" => {
            let inner = type_node.child_by_field_name("type")?;
            let text = inner.utf8_text(source).ok()?;
            let leaf = text.rsplit("::").next().unwrap_or(text);
            match leaf {
                "Path" => Some("&Path"),
                "str" => Some("&str"),
                _ => None,
            }
        }
        // An owned `PathBuf` parameter is a deliberate ownership transfer — the
        // function stores or moves it, so `impl AsRef<Path>` would force a
        // `.to_path_buf()` allocation to recover ownership. Only borrowed path
        // params (`&Path`/`&str`) benefit from the `impl AsRef<Path>` advice.
        _ => None,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_ref_path_param_in_fs_fn() {
        let src = "pub fn load(p: &Path) -> String { fs::read_to_string(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ref_str_param_in_fs_fn() {
        let src = "pub fn load(p: &str) -> Vec<u8> { fs::read(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pathbuf_param_in_fs_fn() {
        // An owned `PathBuf` is an ownership transfer, not an `impl AsRef<Path>`
        // candidate — swapping it would force a `.to_path_buf()` allocation.
        let src = "pub fn load(p: PathBuf) -> String { fs::read_to_string(&p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_moved_pathbuf_param() {
        // Regression for #3736: the function moves/stores the owned `PathBuf`,
        // so it must keep ownership rather than borrow via `impl AsRef<Path>`.
        let src = "pub fn run(cache: PathBuf) -> Source { \
                   let lock_dir = cache.join(\"locks\"); \
                   std::fs::create_dir_all(&lock_dir).unwrap(); \
                   Source::new(cache) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_asref_path_param() {
        // `impl AsRef<Path>` shows up as a different parameter kind
        // (not a `reference_type` of `&Path`/`&str`), so it passes unflagged.
        let src = "fn load<P: AsRef<Path>>(p: P) -> String { fs::read_to_string(p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_fs_function_with_ref_path() {
        // Same signature, but no fs call in the body → not our concern.
        let src = "fn describe(p: &Path) -> String { format!(\"{:?}\", p) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_private_fs_function_with_ref_path() {
        let src = "fn is_merge_state(hg_root: &Path) -> bool { \
                   let path = hg_root.join(\"dirstate\"); \
                   File::open(path).is_ok() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pub_crate_fs_function_with_ref_path() {
        let src = "pub(crate) fn load(p: &Path) -> String { fs::read_to_string(p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pub_fs_function_inside_private_module() {
        // `pub fn` confined to a private `mod` is unreachable from outside the
        // crate, so loosening its signature to `impl AsRef<Path>` buys nothing.
        let src = "mod imp { \
                   pub fn copy_metadata(from: &Path, to: &Path) -> std::io::Result<()> { \
                   let _ = std::fs::metadata(from)?; Ok(()) } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_path_str_param_in_fs_fn() {
        // Regression for #4716: `name` is a log title passed to `.title(...)`,
        // not a path. The fs path is the `PathBuf` field used by `File::create`,
        // so the `&str` param must not be flagged.
        let src = "pub fn build(&self, name: &str) -> std::io::Result<()> { \
                   let file = File::create(&self.qlog_file)?; \
                   let writer = BufWriter::new(file); \
                   qlog.writer(Box::new(writer)).title(Some(name.into())); \
                   Ok(()) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_path_str_param_passed_to_fs_call() {
        // The `&str` param IS the path argument to the fs call, so it stays
        // flagged — the genuine case the rule targets.
        let src = "pub fn save(name: &str, data: &[u8]) -> std::io::Result<()> { \
                   let mut file = File::create(name)?; \
                   file.write_all(data) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_str_param_passed_by_reference_to_fs_call() {
        // `&p` is a direct path argument (the reference wraps the param itself),
        // so the `&str` param stays flagged.
        let src = "pub fn load(p: &str) -> Vec<u8> { fs::read(&p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_str_param_via_asref_chain_to_fs_call() {
        // `p.as_ref()` is a path-view method chain on the param itself — the
        // receiver is the param, so it stays flagged.
        let src = "pub fn load(p: &str) -> std::io::Result<File> { File::open(p.as_ref()) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_str_param_via_aspath_chain_to_fs_call() {
        let src = "pub fn load(p: &str) -> std::io::Result<Metadata> { fs::metadata(p.as_path()) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_str_param_nested_in_inner_path_builder() {
        // Regression for #7658: the fs path is the *return* of
        // `self.thumbnail_path(cas_id, size)`; `cas_id` is only an argument to
        // that inner path-builder, so the `&str` param must not be flagged.
        let src = "pub async fn has_thumbnail(&self, cas_id: &str, size: u32) -> bool { \
                   tokio::fs::metadata(self.thumbnail_path(cas_id, size)).await.is_ok() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_str_param_as_arg_to_join() {
        // `name` is an argument to `dir.join(...)`; the joined `PathBuf` is the
        // path, not `name` itself.
        let src = "pub fn load(name: &str) -> Vec<u8> { fs::read(dir.join(name)).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_str_param_as_arg_to_free_path_builder() {
        // `id` is transformed by the free `build_path` function; its return is
        // the path handed to the fs call, so `id` must not be flagged.
        let src = "pub fn load(id: &str) -> std::io::Result<Metadata> { fs::metadata(build_path(id)) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mut_str_path_param_passed_to_fs_call() {
        // `mut p: &str` binds the same name `p`; the `mut` is unwrapped so the
        // path param is still recognised when passed to the fs call.
        let src = "pub fn load(mut p: &str) -> Vec<u8> { p = p.trim(); fs::read(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_str_param_into_lookalike_callee() {
        // `XFile::open` substring-matches the `File::open` marker but is a
        // different function; the `&str` param fed to it must not be flagged.
        let src = "pub fn build(name: &str) -> Vec<u8> { \
                   let buf = fs::read(&self.path).unwrap(); \
                   let _ = XFile::open(name); buf }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_pub_fs_function_inside_pub_module() {
        // `pub fn` inside a bare-`pub mod` is effectively public, so it stays flagged.
        let src = "pub mod foo { \
                   pub fn find(p: &Path) -> String { fs::read_to_string(p).unwrap() } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pub_fs_function_in_privately_declared_platform_module() {
        // Regression for #6534 (tempfile): `pub fn create(dir: &Path)` lives in
        // `src/file/imp/unix.rs`, which is pulled in by a bare (non-`pub`)
        // `mod platform;` carrying a `#[cfg_attr(…, path = "unix.rs")]` rename in
        // the parent `mod.rs`. The function is therefore unreachable outside the
        // crate, so the `impl AsRef<Path>` suggestion buys nothing.
        let dir = tempfile::TempDir::new().unwrap();
        let imp = dir.path().join("src/file/imp");
        std::fs::create_dir_all(&imp).unwrap();
        std::fs::write(
            imp.join("mod.rs"),
            "#[cfg_attr(any(unix, target_os = \"wasi\"), path = \"unix.rs\")]\n\
             #[cfg_attr(windows, path = \"windows.rs\")]\n\
             mod platform;\n\
             pub use self::platform::*;\n",
        )
        .unwrap();
        let unix = imp.join("unix.rs");
        let source = "pub fn create(dir: &Path) -> std::io::Result<File> { \
                      let _ = std::fs::metadata(dir)?; File::create(dir) }";
        std::fs::write(&unix, source).unwrap();

        assert!(crate::rules::test_helpers::run_rule(&Check, source, &unix).is_empty());
    }

    #[test]
    fn allows_pub_fs_function_in_bare_mod_declared_file() {
        // The stem-matched multi-file case: `mod helper;` (bare, no `#[path]`) in
        // the parent `mod.rs` declares `helper.rs` as a private module, so its
        // top-level `pub fn` is crate-internal.
        let dir = tempfile::TempDir::new().unwrap();
        let util = dir.path().join("src/util");
        std::fs::create_dir_all(&util).unwrap();
        std::fs::write(util.join("mod.rs"), "mod helper;\npub use helper::*;\n").unwrap();
        let helper = util.join("helper.rs");
        let source = "pub fn load(p: &Path) -> Vec<u8> { fs::read(p).unwrap() }";
        std::fs::write(&helper, source).unwrap();

        assert!(crate::rules::test_helpers::run_rule(&Check, source, &helper).is_empty());
    }

    #[test]
    fn flags_pub_fs_function_in_publicly_declared_module() {
        // Negative control: the same file declared by a bare-`pub mod platform;`
        // (still using a `#[path]` rename) is reachable from outside the crate, so
        // its `pub fn` stays flagged. A `pub` declaration does not suppress.
        let dir = tempfile::TempDir::new().unwrap();
        let imp = dir.path().join("src/file/imp");
        std::fs::create_dir_all(&imp).unwrap();
        std::fs::write(
            imp.join("mod.rs"),
            "#[cfg_attr(any(unix, target_os = \"wasi\"), path = \"unix.rs\")]\n\
             pub mod platform;\n\
             pub use self::platform::*;\n",
        )
        .unwrap();
        let unix = imp.join("unix.rs");
        let source = "pub fn create(dir: &Path) -> std::io::Result<File> { \
                      let _ = std::fs::metadata(dir)?; File::create(dir) }";
        std::fs::write(&unix, source).unwrap();

        assert_eq!(crate::rules::test_helpers::run_rule(&Check, source, &unix).len(), 1);
    }
}
