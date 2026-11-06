//! Cross-file named-function duplicate detection.
//!
//! Bodies compare as tokens, ignoring comments and whitespace.
//! Bound names (own name, parameters, locals) compare by first-use rank.
//! Free names, literals and types compare as written.

use rustc_hash::{FxHashMap, FxHashSet};

use rayon::prelude::*;
use tree_sitter::{Node, Parser};

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::{Language, SourceFile};
use crate::parsing::parse_with_grammar;
use crate::project::import_index::collect_pattern_names;

pub const RULE_ID: &str = "no-duplicate-function";

fn is_target_language(lang: Language) -> bool {
    matches!(
        lang,
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Rust
    )
}

fn is_comment_kind(kind: &str) -> bool {
    matches!(kind, "comment" | "line_comment" | "block_comment")
}

/// Whether two files sit under distinct known manifest directories.
/// No manifest, or a shared one, means one package.
fn are_different_packages(a: Option<&std::path::Path>, b: Option<&std::path::Path>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a != b)
}

/// Whether a body name imports a different module per file.
/// Free names match within a bucket; one side suffices.
/// A renamed local can only exempt wrongly.
fn any_divergent_import_source(
    a: &FnEntry,
    b: &FnEntry,
    import_maps: &[FxHashMap<String, String>],
) -> bool {
    let map_a = &import_maps[a.file_idx];
    let map_b = &import_maps[b.file_idx];
    a.body_idents.iter().any(|name| {
        matches!((map_a.get(name), map_b.get(name)), (Some(sa), Some(sb)) if sa != sb)
    })
}

/// Whether a specifier resolves inside the importing package or crate.
fn is_relative_specifier(spec: Option<&String>) -> bool {
    spec.is_some_and(|s| {
        s.starts_with('.')
            || s.starts_with("crate::")
            || s.starts_with("self::")
            || s.starts_with("super::")
    })
}

/// Whether either body name imports a relative module specifier.
/// A relative path resolves per package, so targets differ.
/// Free names match within a bucket; one side suffices.
fn any_relative_import_dependency(
    a: &FnEntry,
    b: &FnEntry,
    import_maps: &[FxHashMap<String, String>],
) -> bool {
    let map_a = &import_maps[a.file_idx];
    let map_b = &import_maps[b.file_idx];
    a.body_idents
        .iter()
        .any(|name| is_relative_specifier(map_a.get(name)) || is_relative_specifier(map_b.get(name)))
}

/// Whether a body name resolves to diverging module-local declarations.
/// The own name is skipped: export status varies.
/// Free names match within a bucket; one side suffices.
fn any_divergent_module_local(
    a: &FnEntry,
    b: &FnEntry,
    module_local_decls: &[FxHashMap<String, Vec<u8>>],
) -> bool {
    let locals_a = &module_local_decls[a.file_idx];
    let locals_b = &module_local_decls[b.file_idx];
    a.body_idents
        .iter()
        .filter(|name| name.as_str() != a.name)
        .any(|name| match (locals_a.get(name), locals_b.get(name)) {
            (Some(fa), Some(fb)) => fa != fb,
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        })
}

/// Whether the declaration at `decl` is `export`ed: a TS/JS `export` keyword
/// wraps the declaration in an `export_statement` ancestor. Walks up because a
/// `const` binding sits two levels under the `export_statement` (via its
/// `lexical_declaration`). A detached re-export (`export { foo }`) reads as
/// private here; the cross-package exemption then errs toward exempting, never
/// toward a missed duplicate.
fn is_exported(decl: Node) -> bool {
    let mut cur = decl.parent();
    while let Some(node) = cur {
        match node.kind() {
            "export_statement" => return true,
            "lexical_declaration" | "variable_declaration" => cur = node.parent(),
            _ => return false,
        }
    }
    false
}

/// Whether `kind` is a function-like scope — a node that introduces its own
/// binding scope and body. A declaration whose ancestor chain contains one of
/// these is nested inside another function rather than sitting at module level.
fn is_function_scope_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "generator_function"
            | "generator_function_declaration"
            | "method_definition"
    )
}

/// Whether the declaration at `decl` is nested inside another function's body —
/// i.e. it has a function-like ancestor. A module-top-level function cannot
/// reference enclosing-function-scope bindings; a nested one can.
fn is_nested_function(decl: Node) -> bool {
    let mut cur = decl.parent();
    while let Some(node) = cur {
        if is_function_scope_kind(node.kind()) {
            return true;
        }
        cur = node.parent();
    }
    false
}

/// Whether any body free variable resolves to neither an import nor a top-level
/// module-local declaration — meaning it is bound in an enclosing function scope
/// (a closure capture). Applied only to nested functions: for a module-level
/// function every such free var is a global, which does not affect hoistability.
/// The function's own name is excluded (a nested recursive call is not a
/// capture). `body_idents` already excludes the function's own parameters.
fn has_enclosing_scope_capture(
    entry: &FnEntry,
    import_map: &FxHashMap<String, String>,
    module_local_decls: &FxHashMap<String, Vec<u8>>,
) -> bool {
    entry.body_idents.iter().any(|name| {
        name != &entry.name
            && !import_map.contains_key(name)
            && !module_local_decls.contains_key(name)
    })
}

/// A named function eligible to be compared against others.
struct FnEntry {
    file_idx: usize,
    name: String,
    line: usize,
    column: usize,
    span: (usize, usize),
    /// Whether the declaration is `export`ed. A file-private (unexported) helper
    /// has no importable surface, so the "extract to a shared module" remedy does
    /// not apply across a package boundary.
    is_exported: bool,
    /// Exact token fingerprint of the head and body.
    signature: Vec<u8>,
    /// `signature` with bound names as first-use ranks; the bucket key.
    renamed_signature: Vec<u8>,
    /// Identifier names referenced in the body that are not the function's own
    /// parameters — an over-approximation of its free variables. Compared against
    /// each file's import map so a same-named alias resolving to a different
    /// module specifier per file is not treated as the same call.
    body_idents: FxHashSet<String>,
}

/// Catalog entry only; `main.rs` runs the detector itself.
pub fn register() -> crate::rules::RuleDef {
    crate::rules::RuleDef {
        meta: crate::rules::meta::RuleMeta {
            id: RULE_ID,
            description: "A named function must not be copied into another file, verbatim or with renamed parameters, locals or name.",
            remediation: "Extract the function into a shared module and import it from both call sites.",
            severity: Severity::Error,
            doc_url: None,
            categories: &["code-quality"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        backends: vec![],
    }
}

/// A copy shared with `reference` reports on the `files` side.
#[must_use]
pub fn lint_files(
    files: &[&SourceFile],
    reference: &[&SourceFile],
    config: &Config,
) -> Vec<Diagnostic> {
    // Sample/example/docs/fixture/fuzz-harness dirs hold intentionally
    // self-contained, duplicated code; generated files are machine-emitted. Drop
    // both so a relaxed file is neither reported nor used as a canonical match.
    let is_checked = |f: &&SourceFile| {
        is_target_language(f.language)
            && !crate::rules::file_ctx::scan_path(&f.path).is_relaxed_dir
            && !crate::rules::file_ctx::is_generated_path(&f.path)
    };
    let mut checked: Vec<&SourceFile> = reference.iter().copied().filter(is_checked).collect();
    let reference_count = checked.len();
    checked.extend(files.iter().copied().filter(is_checked));
    let files = checked;

    if files.len() < 2 {
        return vec![];
    }

    // Nearest crate or package manifest, one walk-up per file.
    // This runs beside `ProjectCtx::load`: no cached accessor.
    let package_dirs: Vec<Option<std::path::PathBuf>> = files
        .iter()
        .map(|f| {
            let manifest = if f.language == Language::Rust {
                "Cargo.toml"
            } else {
                "package.json"
            };
            f.path
                .parent()
                .and_then(|dir| crate::project::walk_up_finding(dir, manifest))
        })
        .collect();

    // Cross-language rule: its single knob lives in a non-per-language
    // `[rules.<id>]` block, so the `Language` passed to the lookup is immaterial.
    let min_body_tokens = config.threshold(RULE_ID, "min_body_tokens", Language::TypeScript);

    // Each file yields its functions, its import map (local-name →
    // module-specifier), and its module-local declarations (top-level
    // non-exported name → signature fingerprint). `collect` preserves file
    // order, so `import_maps[idx]` and `module_local_decls[idx]` belong to the
    // file whose functions carry `file_idx == idx`.
    let per_file: Vec<FileExtraction> = files
        .par_iter()
        .enumerate()
        .map_init(Parser::new, |parser, (idx, file)| {
            extract_functions(parser, file, idx, min_body_tokens)
        })
        .collect();
    let mut entries: Vec<FnEntry> = Vec::new();
    let mut import_maps: Vec<FxHashMap<String, String>> = Vec::with_capacity(per_file.len());
    let mut module_local_decls: Vec<FxHashMap<String, Vec<u8>>> =
        Vec::with_capacity(per_file.len());
    for (file_entries, imports, locals) in per_file {
        entries.extend(file_entries);
        import_maps.push(imports);
        module_local_decls.push(locals);
    }

    let mut buckets: FxHashMap<&[u8], Vec<usize>> = FxHashMap::default();
    for (i, entry) in entries.iter().enumerate() {
        buckets
            .entry(entry.renamed_signature.as_slice())
            .or_default()
            .push(i);
    }

    let mut diags = Vec::new();
    for members in buckets.values() {
        if members.len() < 2 {
            continue;
        }
        // Inter-file only, in path then line order.
        // Reference files sort first, so the scanned copy reports.
        let mut ordered = members.clone();
        ordered.sort_by(|&a, &b| {
            let (ea, eb) = (&entries[a], &entries[b]);
            (ea.file_idx >= reference_count)
                .cmp(&(eb.file_idx >= reference_count))
                .then(files[ea.file_idx].path.cmp(&files[eb.file_idx].path))
                .then(ea.line.cmp(&eb.line))
        });
        // An exact copy reports against its first twin.
        // Any other copy reports against the first member.
        // One diagnostic per file and twin.
        let mut seen: FxHashSet<(usize, Option<usize>)> = FxHashSet::default();
        let mut pairs = Vec::new();
        for &i in &ordered {
            let entry = &entries[i];
            let twin = ordered
                .iter()
                .copied()
                .find(|&j| entries[j].name == entry.name && entries[j].signature == entry.signature)
                .filter(|&j| entries[j].file_idx != entry.file_idx);
            let canonical = twin.unwrap_or(ordered[0]);
            if entries[canonical].file_idx != entry.file_idx && seen.insert((entry.file_idx, twin))
            {
                pairs.push((i, canonical));
            }
        }

        for (m, c) in pairs {
            let entry = &entries[m];
            let canonical = &entries[c];
            // Two executable test specs may carry the same named helper
            // (`uniqueName`, a per-spec render setup) without it being a smell:
            // specs are meant to stand alone, so extracting shared state into a
            // common module would couple them. Mirrors the clone detector's
            // test-spec-sibling exemption. A duplicate involving shared test
            // infrastructure (`test-helpers/`, `__mocks__/`) is still reported —
            // there extraction is the right fix.
            if crate::clone_detection::are_test_spec_siblings(
                &files[entry.file_idx].path,
                &files[canonical.file_idx].path,
            ) {
                continue;
            }
            // A file-private helper copied into a *different* npm package
            // (different nearest `package.json`) is often deliberate: extracting
            // it into a shared package would either expose an internal helper as
            // public API or create a dependency cycle the packages avoid by
            // staying self-contained. With no importable surface, "extract and
            // import" is not actionable. Exported duplicates still flag — there
            // hoisting to a shared package is the right fix — and same-package
            // private duplicates still flag, where a local import is trivial.
            let is_rust = |e: &FnEntry| files[e.file_idx].language == Language::Rust;
            // A private Rust helper stays hoistable into a shared crate.
            let has_rust = is_rust(entry) || is_rust(canonical);
            let is_private_js_pair = !entry.is_exported && !canonical.is_exported && !has_rust;
            if is_private_js_pair
                && are_different_packages(
                    package_dirs[entry.file_idx].as_deref(),
                    package_dirs[canonical.file_idx].as_deref(),
                )
            {
                continue;
            }
            // Platform/runtime entry files (h3's `_entries/{bun,node,deno}.ts`)
            // carry byte-identical bodies that call a same-named local alias
            // resolving to a different runtime package per file — `import { serve
            // as srvxServe } from "srvx/bun"` vs `"srvx/node"`. The platform is
            // encoded in the import path, not a parameter, so the bodies cannot be
            // hoisted into one shared module: they are not interchangeable. Skip
            // when a body identifier resolves to imports with diverging module
            // specifiers across the two files.
            if any_divergent_import_source(entry, canonical, &import_maps) {
                continue;
            }
            // Driver adapters (`@prisma/adapter-neon`,
            // `@prisma/adapter-better-sqlite3`) each export a byte-identical
            // `convertDriverError` whose body delegates to a module-LOCAL
            // `mapDriverError`/`isDriverError` — a top-level non-exported
            // declaration defined differently per file (PostgreSQL vs SQLite error
            // mapping). The shared name resolves to a different implementation in
            // each file, so the bodies cannot be hoisted into one shared module:
            // the callee would rebind to the shared version and silently break each
            // adapter. Skip when a body identifier resolves to a module-local
            // declaration whose implementation diverges across the two files. A
            // body delegating only to imported, built-in, or byte-identical
            // module-local symbols carries no such divergence and stays flagged —
            // it is genuinely hoistable.
            if any_divergent_module_local(entry, canonical, &module_local_decls) {
                continue;
            }
            // Framework-adapter packages (`@xstate/svelte`, `@xstate/solid`,
            // `@xstate/vue`) each export the same thin public hook (`useMachine`,
            // `useStore`, `useAtomState`) whose byte-identical body delegates to a
            // package-LOCAL `useActor` imported via a relative specifier
            // (`./useActor`). A relative specifier resolves to a different file in
            // each package — its specifier text being identical does not make the
            // targets identical — so the bodies cannot be hoisted into one shared
            // module: each depends on its own framework-specific implementation.
            // Skip when the two functions live in different packages and a body
            // identifier resolves to a relative import in either file. A pure
            // cross-package duplicate with no relative-import delegation (e.g.
            // `shallowEqual`) stays flagged — it is genuinely hoistable.
            if are_different_packages(
                package_dirs[entry.file_idx].as_deref(),
                package_dirs[canonical.file_idx].as_deref(),
            ) && any_relative_import_dependency(entry, canonical, &import_maps)
            {
                continue;
            }
            let is_exact = entry.name == canonical.name && entry.signature == canonical.signature;
            let message = if is_exact {
                format!(
                    "Duplicate function `{}` — an identical definition is in `{}` at line {}. \
                     Extract it into a shared module and import it from both call sites.",
                    entry.name,
                    files[canonical.file_idx].path.display(),
                    canonical.line,
                )
            } else {
                format!(
                    "Function `{}` is a renamed copy of `{}` in `{}` at line {}. \
                     Extract it into a shared module and import it from both call sites.",
                    entry.name,
                    canonical.name,
                    files[canonical.file_idx].path.display(),
                    canonical.line,
                )
            };
            diags.push(Diagnostic {
                path: std::sync::Arc::from(files[entry.file_idx].path.as_path()),
                line: entry.line,
                column: entry.column,
                rule_id: RULE_ID.into(),
                message,
                severity: Severity::Error,
                span: Some(entry.span),
            });
        }
    }

    diags.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    diags
}

/// A file's functions, import map and module-local declaration fingerprints.
type FileExtraction = (Vec<FnEntry>, FxHashMap<String, String>, FxHashMap<String, Vec<u8>>);

fn extract_functions(
    parser: &mut Parser,
    file: &SourceFile,
    file_idx: usize,
    min_body_tokens: usize,
) -> FileExtraction {
    let Ok(source) = std::fs::read_to_string(&file.path) else {
        return (Vec::new(), FxHashMap::default(), FxHashMap::default());
    };
    // Minified/bundled files (e.g. `*-min.js`, `*.min.js`, webpack bundles, or a
    // multi-KB single payload line) are machine-emitted build artifacts whose
    // inlined/copy-pasted helpers flood this pass. Skip them via the same
    // predicate the per-file engine uses, so cross-file detection stays scoped to
    // authored source. (Reads source here because the name marker alone misses
    // unmarked bundles that the content heuristic catches.)
    if crate::rules::file_ctx::is_generated_content(&source)
        || crate::rules::file_ctx::scan_minified(&file.path, &source)
    {
        return (Vec::new(), FxHashMap::default(), FxHashMap::default());
    }
    let Some(tree) = parse_with_grammar(parser, file.language, source.as_bytes()) else {
        return (Vec::new(), FxHashMap::default(), FxHashMap::default());
    };
    let bytes = source.as_bytes();

    let mut module_local_decls = FxHashMap::default();
    collect_module_local_decls(tree.root_node(), bytes, &mut module_local_decls);

    // Each entry is paired with whether its declaration is nested inside another
    // function's body, decided during the walk while the parent chain is at hand.
    let mut entries: Vec<(FnEntry, bool)> = Vec::new();
    let mut import_map = FxHashMap::default();
    if file.language == Language::Rust {
        // Inline `crate::`/`super::` paths read as crate-relative imports.
        import_map.insert("crate".to_string(), "crate::".to_string());
        import_map.insert("super".to_string(), "super::".to_string());
    }
    let mut cursor = tree.walk();
    'walk: loop {
        let node = cursor.node();
        if node.kind() == "import_statement" {
            collect_import(node, bytes, &mut import_map);
        }
        if node.kind() == "use_declaration"
            && let Some(argument) = node.child_by_field_name("argument")
        {
            collect_use(argument, "", bytes, &mut import_map);
        }
        if let Some((name, decl, sig_node, body)) = named_function_parts(node)
            && let Some(entry) =
                build_entry(name, decl, sig_node, body, bytes, file_idx, min_body_tokens)
            {
                entries.push((entry, is_nested_function(decl)));
            }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }

    // A nested function that captures an enclosing-scope binding is not
    // hoistable: a body free variable that resolves to neither an import nor a
    // top-level module-local declaration is bound in an enclosing function scope
    // (a closure capture, like a `setup(props)` parameter or a `const`
    // destructured from a per-component call). Extracting such a function into a
    // shared module would require passing the capture as a parameter, changing
    // its signature, so it is not the cross-file duplicate this rule targets.
    // Drop it before any comparison. (A referenced JS global absent from both
    // maps also triggers this and is skipped — an acceptable, FP-safe miss,
    // confined to nested functions.) The import map is fully built here, so a
    // free var is checked against every import regardless of source order.
    let entries: Vec<FnEntry> = entries
        .into_iter()
        .filter(|(entry, is_nested)| {
            !(*is_nested && has_enclosing_scope_capture(entry, &import_map, &module_local_decls))
        })
        .map(|(entry, _)| entry)
        .collect();

    (entries, import_map, module_local_decls)
}

/// Top-level, non-`export`ed declarations that resolve to a module-local
/// implementation, as `name → signature fingerprint`: a `function foo() {…}`
/// declaration or a `const foo = (…) => {…}` / `const foo = function (…) {…}`
/// binding (and the `let`/`var` forms). The same identifier can name a
/// completely different implementation per file, so a body delegating to one
/// may not be interchangeable across files. The fingerprint lets the caller
/// tell an actually-divergent callee from a byte-identical one (which is
/// hoistable). `export`ed declarations are excluded: they carry an importable
/// surface and are hoistable alongside, not a hidden divergence (an
/// `export_statement` wrapper is not matched here, so its declarations are
/// skipped). Only direct children of the program are scanned, so a binding
/// nested inside a function body — which cannot collide with another file's
/// top-level name — is ignored.
/// A Rust `fn` counts only without `pub`.
fn collect_module_local_decls(root: Node, source: &[u8], out: &mut FxHashMap<String, Vec<u8>>) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration" => insert_decl_fingerprint(child, source, out),
            "function_item"
                if !child
                    .named_children(&mut child.walk())
                    .any(|c| c.kind() == "visibility_modifier") =>
            {
                insert_decl_fingerprint(child, source, out);
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut inner = child.walk();
                for declarator in child.named_children(&mut inner) {
                    insert_decl_fingerprint(declarator, source, out);
                }
            }
            _ => {}
        }
    }
}

/// Insert `declared-name → signature fingerprint` for `node` when it is a named
/// function form — a `function_declaration`, or a `variable_declarator` bound to
/// an arrow/function expression. Reuses `named_function_parts` so the same
/// binding-kind rule that selects entries also selects module-local callees;
/// other declarators (e.g. `const MAX = 5`, a destructuring pattern) carry no
/// callee that can diverge under a shared name and are skipped.
/// A Rust `function_item` is a named function form too.
fn insert_decl_fingerprint(node: Node, source: &[u8], out: &mut FxHashMap<String, Vec<u8>>) {
    let Some((name, _, sig_node, body)) = named_function_parts(node) else {
        return;
    };
    let Ok(text) = name.utf8_text(source) else {
        return;
    };
    if text.is_empty() {
        return;
    }
    let (fingerprint, _) = signature_fingerprint(sig_node, body, source, &FxHashSet::default());
    out.insert(text.to_string(), fingerprint);
}

/// Record every local binding an `import` statement introduces as
/// `local-name → module-specifier`. Default (`import d from "m"`), namespace
/// (`import * as n from "m"`), and named (`import { a as b } from "m"`) clauses
/// are covered; for an aliased named import the *local* name (`b`) is the key.
fn collect_import(node: Node, source: &[u8], map: &mut FxHashMap<String, String>) {
    let Some(specifier) = import_specifier_text(node, source) else {
        return;
    };
    let Some(clause) = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "import_clause")
    else {
        return;
    };
    let mut cursor = clause.walk();
    for child in clause.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => insert_local(child, source, &specifier, map),
            "namespace_import" => {
                if let Some(id) = child
                    .named_children(&mut child.walk())
                    .find(|c| c.kind() == "identifier")
                {
                    insert_local(id, source, &specifier, map);
                }
            }
            "named_imports" => {
                let mut nested = child.walk();
                for spec in child.named_children(&mut nested) {
                    if spec.kind() != "import_specifier" {
                        continue;
                    }
                    // `{ a }` → local `a`; `{ a as b }` → local `b`. The local
                    // binding is the last identifier of the specifier.
                    if let Some(local) = spec
                        .named_children(&mut spec.walk())
                        .filter(|c| c.kind() == "identifier")
                        .last()
                    {
                        insert_local(local, source, &specifier, map);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Insert `local-name → specifier` for an identifier node, skipping empties.
fn insert_local(id: Node, source: &[u8], specifier: &str, map: &mut FxHashMap<String, String>) {
    if let Ok(name) = id.utf8_text(source)
        && !name.is_empty()
    {
        map.insert(name.to_string(), specifier.to_string());
    }
}

/// Record each name a Rust `use` binds, with its path.
fn collect_use(node: Node, prefix: &str, source: &[u8], map: &mut FxHashMap<String, String>) {
    let full_path = |path: Node| {
        let text = path.utf8_text(source).unwrap_or("");
        if prefix.is_empty() {
            text.to_string()
        } else {
            format!("{prefix}::{text}")
        }
    };
    match node.kind() {
        "identifier" | "scoped_identifier" => {
            let local = node.child_by_field_name("name").unwrap_or(node);
            insert_local(local, source, &full_path(node), map);
        }
        "use_as_clause" => {
            if let (Some(path), Some(alias)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("alias"),
            ) {
                insert_local(alias, source, &full_path(path), map);
            }
        }
        "scoped_use_list" => {
            let nested = node
                .child_by_field_name("path")
                .map_or_else(|| prefix.to_string(), full_path);
            if let Some(list) = node.child_by_field_name("list") {
                collect_use(list, &nested, source, map);
            }
        }
        "use_list" => {
            for child in node.named_children(&mut node.walk()) {
                collect_use(child, prefix, source, map);
            }
        }
        _ => {}
    }
}

/// The unquoted module specifier of an `import` statement — its `string` child.
fn import_specifier_text(node: Node, source: &[u8]) -> Option<String> {
    let str_node = node
        .named_children(&mut node.walk())
        .find(|c| c.kind() == "string")?;
    let raw = str_node.utf8_text(source).ok()?;
    Some(
        raw.trim_matches(|c| c == '\'' || c == '"' || c == '`')
            .to_string(),
    )
}

/// The `(name, declaration, signature node, body)` of a named function.
/// Forms: TS `function` declarations and `const` bindings, Rust `fn` items.
/// Bodiless forms are skipped.
fn named_function_parts<'a>(node: Node<'a>) -> Option<(Node<'a>, Node<'a>, Node<'a>, Node<'a>)> {
    match node.kind() {
        "function_declaration" | "function_item" => {
            let name = node.child_by_field_name("name")?;
            let body = node.child_by_field_name("body")?;
            Some((name, node, node, body))
        }
        "variable_declarator" => {
            let name = node.child_by_field_name("name")?;
            if name.kind() != "identifier" {
                return None;
            }
            let value = node.child_by_field_name("value")?;
            if !matches!(value.kind(), "arrow_function" | "function_expression") {
                return None;
            }
            let body = value.child_by_field_name("body")?;
            Some((name, node, value, body))
        }
        _ => None,
    }
}

/// Token fingerprint of type parameters, parameters, return type and body.
/// Names in `bound` become first-use ranks; empty means exact.
/// Returns it with the body token count.
/// A method's `impl` or trait header comes first.
fn signature_fingerprint(
    sig_node: Node,
    body: Node,
    source: &[u8],
    bound: &FxHashSet<String>,
) -> (Vec<u8>, usize) {
    let mut signature = Vec::new();
    let mut seen = Vec::new();
    let mut head_count = 0;
    let owner = sig_node
        .parent()
        .filter(|list| list.kind() == "declaration_list")
        .and_then(|list| list.parent())
        .filter(|owner| matches!(owner.kind(), "impl_item" | "trait_item"));
    if let Some(owner) = owner {
        for child in owner.children(&mut owner.walk()) {
            if child.kind() != "declaration_list" {
                collect_body_tokens(
                    child,
                    source,
                    bound,
                    &mut seen,
                    &mut signature,
                    &mut head_count,
                );
            }
        }
    }
    for field in ["type_parameters", "parameters", "return_type"] {
        if let Some(head) = sig_node.child_by_field_name(field) {
            collect_body_tokens(
                head,
                source,
                bound,
                &mut seen,
                &mut signature,
                &mut head_count,
            );
        }
    }
    signature.extend_from_slice(b"\x00body\x00");

    let mut body_token_count = 0;
    collect_body_tokens(
        body,
        source,
        bound,
        &mut seen,
        &mut signature,
        &mut body_token_count,
    );
    (signature, body_token_count)
}

/// Names bound by parameters, declarators, `for`/`catch` heads, nested functions.
fn collect_bound_names(sig_node: Node, source: &[u8], out: &mut Vec<String>) {
    let mut stack = vec![sig_node];
    while let Some(node) = stack.pop() {
        if let Some(params) = node.child_by_field_name("parameters") {
            for param in params.named_children(&mut params.walk()) {
                let pattern = param.child_by_field_name("pattern").unwrap_or(param);
                collect_pattern_names(pattern, source, out);
            }
        }
        let binding = match node.kind() {
            "variable_declarator"
            | "function_declaration"
            | "function_item"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration" => node.child_by_field_name("name"),
            "arrow_function" | "catch_clause" => node.child_by_field_name("parameter"),
            "let_declaration" | "for_expression" => node.child_by_field_name("pattern"),
            // `for (x of xs)` without `const` assigns an outer name.
            "for_in_statement" if node.child_by_field_name("kind").is_some() => {
                node.child_by_field_name("left")
            }
            _ => None,
        };
        if let Some(binding) = binding {
            collect_pattern_names(binding, source, out);
        }
        stack.extend(node.named_children(&mut node.walk()));
    }
}

fn build_entry(
    name: Node,
    decl: Node,
    sig_node: Node,
    body: Node,
    source: &[u8],
    file_idx: usize,
    min_body_tokens: usize,
) -> Option<FnEntry> {
    let name_text = source.get(name.start_byte()..name.end_byte())?;
    let name_str = std::str::from_utf8(name_text).ok()?.to_string();

    let (signature, body_token_count) =
        signature_fingerprint(sig_node, body, source, &FxHashSet::default());
    if body_token_count < min_body_tokens {
        return None;
    }
    let mut bound_names = vec![name_str.clone()];
    collect_bound_names(sig_node, source, &mut bound_names);
    let bound: FxHashSet<String> = bound_names.into_iter().collect();
    let (renamed_signature, _) = signature_fingerprint(sig_node, body, source, &bound);

    // Free-ish variables: identifiers used in the body minus the function's own
    // parameters, so a parameter named like an import is not mistaken for a
    // reference to that import. Property accesses (`property_identifier`) and
    // type-level names (`type_identifier`) are different node kinds and are not
    // captured here.
    let mut params_idents = FxHashSet::default();
    if let Some(params) = sig_node.child_by_field_name("parameters") {
        collect_identifiers(params, source, &mut params_idents);
    }
    // A path root in a parameter type binds nothing.
    params_idents.remove("crate");
    params_idents.remove("super");
    let mut body_idents = FxHashSet::default();
    collect_identifiers(body, source, &mut body_idents);
    body_idents.retain(|n| !params_idents.contains(n));

    let pos = name.start_position();
    Some(FnEntry {
        file_idx,
        name: name_str,
        line: pos.row + 1,
        column: pos.column + 1,
        span: (decl.start_byte(), decl.end_byte() - decl.start_byte()),
        is_exported: is_exported(decl),
        signature,
        renamed_signature,
        body_idents,
    })
}

/// Insert the text of every `identifier` leaf under `node` into `out`. Property
/// accesses (`property_identifier`) and type references (`type_identifier`) are
/// distinct node kinds and so are not collected.
fn collect_identifiers(node: Node, source: &[u8], out: &mut FxHashSet<String>) {
    // A Rust path root names the crate-relative import key.
    let is_path_root = matches!(node.kind(), "crate" | "super")
        && node
            .parent()
            .is_some_and(|p| matches!(p.kind(), "scoped_identifier" | "scoped_type_identifier"));
    if node.kind() == "identifier" || is_path_root {
        if let Ok(text) = node.utf8_text(source)
            && !text.is_empty()
        {
            out.insert(text.to_string());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, source, out);
    }
}

/// Append every leaf token under `node` (comments excluded) to `sig` as
/// `kind_id` + text, separated so two distinct token streams can never collide.
/// Counts the tokens so the caller can gate trivial bodies.
fn collect_body_tokens<'s>(
    node: Node,
    source: &'s [u8],
    bound: &FxHashSet<String>,
    seen: &mut Vec<&'s [u8]>,
    sig: &mut Vec<u8>,
    count: &mut usize,
) {
    if node.is_error() || node.is_missing() {
        return;
    }
    if node.child_count() == 0 {
        if is_comment_kind(node.kind()) {
            return;
        }
        let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        sig.extend_from_slice(&node.kind_id().to_le_bytes());
        sig.push(0);
        let is_bound = node.kind() == "identifier"
            && std::str::from_utf8(text).is_ok_and(|name| bound.contains(name));
        if is_bound {
            // Identifiers never start with 0x01, so ranks cannot collide.
            let rank = seen.iter().position(|s| *s == text).unwrap_or(seen.len());
            if rank == seen.len() {
                seen.push(text);
            }
            sig.push(1);
            sig.extend_from_slice(&rank.to_le_bytes());
        } else {
            sig.extend_from_slice(text);
        }
        sig.push(0);
        *count += 1;
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_body_tokens(child, source, bound, seen, sig, count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, content: &str) -> SourceFile {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        let language = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Language::Rust,
            Some("tsx") => Language::Tsx,
            Some("js") => Language::JavaScript,
            _ => Language::TypeScript,
        };
        SourceFile { path, language }
    }

    fn run(files: &[&SourceFile]) -> Vec<Diagnostic> {
        lint_files(files, &[], &Config::default())
    }

    /// Drop a `package.json` (so a file under it resolves to that package root)
    /// at `dir/rel/package.json` with the given npm `name`.
    fn write_pkg(dir: &tempfile::TempDir, rel: &str, name: &str) {
        let path = dir.path().join(rel).join("package.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{{ \"name\": \"{name}\" }}")).unwrap();
    }

    // A file-private helper (unexported), mirroring tonal's `ascR`/`descR`.
    const PRIVATE_HELPER: &str = "\
function ascR(b: number, n: number): number[] {
  const a = [];
  for (; n--; a[n] = n + b);
  return a;
}
";

    // The exact copy-paste from saurenya MR 1292.
    const CELL_TO_STRING: &str = "\
function cellToString(cell: unknown): string {
  if (typeof cell === \"string\") return cell;
  if (typeof cell === \"number\" || typeof cell === \"boolean\") return String(cell);
  return \"\";
}
";

    #[test]
    fn flags_duplicate_named_function_across_files() {
        // Regression (saurenya MR 1292): `cellToString` pasted verbatim into two
        // import readers. ~30 body tokens — under no-clones' 100-token window.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "direct-reader.ts", &format!("export const x = 1;\n{CELL_TO_STRING}"));
        let b = write(&dir, "hipra.ts", &format!("export const y = 2;\n{CELL_TO_STRING}"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "the pasted function must be flagged");
        assert_eq!(diags[0].rule_id, RULE_ID);
        assert!(
            diags[0]
                .message
                .starts_with("Duplicate function `cellToString`")
        );
        // Canonical is the lexicographically-first path; hipra.ts reports it.
        assert!(diags[0].path.ends_with("hipra.ts"));
        assert!(diags[0].message.contains("direct-reader.ts"));
    }

    #[test]
    fn comment_inside_body_is_ignored() {
        // Whitespace and an extra comment inside one copy must not hide the dup.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let with_comment = "\
function cellToString(cell: unknown): string {
  // explain the primitive cases
  if (typeof cell === \"string\") return cell;
  if (typeof cell === \"number\" || typeof cell === \"boolean\")    return String(cell);
  return \"\";
}
";
        let b = write(&dir, "b.ts", with_comment);
        assert_eq!(run(&[&a, &b]).len(), 1, "comments and whitespace are not part of the body");
    }

    #[test]
    fn same_name_different_body_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let variant = "\
function cellToString(cell: unknown): string {
  if (typeof cell === \"string\") return cell.trim();
  if (typeof cell === \"number\" || typeof cell === \"boolean\") return String(cell);
  if (cell == null) return \"n/a\";
  return \"\";
}
";
        let b = write(&dir, "b.ts", variant);
        assert!(run(&[&a, &b]).is_empty(), "a different body is not a duplicate");
    }

    #[test]
    fn different_name_same_body_flagged_as_renamed_copy() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let renamed = CELL_TO_STRING.replace("cellToString", "stringifyCell");
        let b = write(&dir, "b.ts", &renamed);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a renamed function is still a copy");
        assert!(diags[0].path.ends_with("b.ts"));
        assert!(diags[0].message.contains("renamed copy of `cellToString`"));
    }

    #[test]
    fn renamed_copy_is_flagged_and_names_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let original = "\
export function cellToString(cell: unknown): string {
  const kind = typeof cell;
  if (kind === \"number\" || kind === \"boolean\") return String(cell);
  return \"\";
}
";
        let renamed = original
            .replace("cellToString", "stringifyCell")
            .replace("cell", "value")
            .replace("kind", "sort");
        let a = write(&dir, "a.ts", original);
        let b = write(&dir, "b.ts", &renamed);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a copy with renamed bound names is flagged");
        assert!(diags[0].path.ends_with("b.ts"));
        assert!(diags[0].message.contains("renamed copy of `cellToString`"));
        assert!(diags[0].message.contains("a.ts"));
    }

    #[test]
    fn renamed_copy_calling_another_function_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let original = "\
export function cellToString(cell: unknown): string {
  const kind = typeof cell;
  if (kind === \"number\" || kind === \"boolean\") return String(cell);
  return \"\";
}
";
        let other = "\
export function cellToNumber(item: unknown): string {
  const sort = typeof item;
  if (sort === \"number\" || sort === \"boolean\") return Number(item);
  return \"\";
}
";
        let a = write(&dir, "a.ts", original);
        let b = write(&dir, "b.ts", other);
        assert!(run(&[&a, &b]).is_empty(), "another callee is not a copy");
    }

    #[test]
    fn renamed_copy_with_another_default_callee_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let original = "\
export function f(cb = handleA, value: number) {
  const total = value + 1;
  if (total > 10) return cb(total);
  return total;
}
";
        let other = "\
export function g(fn = handleB, amount: number) {
  const sum = amount + 1;
  if (sum > 10) return fn(sum);
  return sum;
}
";
        let a = write(&dir, "a.ts", original);
        let b = write(&dir, "b.ts", other);
        assert!(run(&[&a, &b]).is_empty(), "another default callee differs");
    }

    #[test]
    fn renamed_copy_with_another_literal_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let original = "\
export function cellToString(cell: unknown): string {
  const kind = typeof cell;
  if (kind === \"number\" || kind === \"boolean\") return String(cell);
  return \"\";
}
";
        let other = "\
export function stringifyCell(value: unknown): string {
  const sort = typeof value;
  if (sort === \"number\" || sort === \"bigint\") return String(value);
  return \"\";
}
";
        let a = write(&dir, "a.ts", original);
        let b = write(&dir, "b.ts", other);
        assert!(run(&[&a, &b]).is_empty(), "another literal is not a copy");
    }

    #[test]
    fn one_name_split_into_two_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let original = "\
export function cellToString(cell: unknown): string {
  const kind = typeof cell;
  if (kind === \"number\" || kind === \"boolean\") return String(cell);
  return kind;
}
";
        let other = "\
export function stringifyCell(value: unknown): string {
  const sort = typeof value;
  if (sort === \"number\" || sort === \"boolean\") return String(value);
  return value;
}
";
        let a = write(&dir, "a.ts", original);
        let b = write(&dir, "b.ts", other);
        assert!(run(&[&a, &b]).is_empty(), "one name became two names");
    }

    #[test]
    fn renamed_trivial_function_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", "function a(x) { return x; }\n");
        let b = write(&dir, "b.ts", "function b(y) { return y; }\n");
        assert!(run(&[&a, &b]).is_empty(), "a trivial function is ignored");
    }

    #[test]
    fn trivial_function_below_threshold_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", "export function noop() {}\n");
        let b = write(&dir, "b.ts", "export function noop() {}\n");
        assert!(run(&[&a, &b]).is_empty(), "a body under min_body_tokens is ignored");
    }

    #[test]
    fn flags_duplicate_arrow_const() {
        let dir = tempfile::tempdir().unwrap();
        let arrow = CELL_TO_STRING
            .replace("function cellToString(cell: unknown): string {", "const cellToString = (cell: unknown): string => {")
            .replace("\n}\n", "\n};\n");
        let a = write(&dir, "a.ts", &arrow);
        let b = write(&dir, "b.ts", &arrow);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a duplicated arrow-const function is flagged");
        assert!(diags[0].message.contains("`cellToString`"));
    }

    #[test]
    fn sibling_test_specs_not_flagged() {
        // Two executable specs may carry the same named helper without it being a
        // smell — specs stand alone. Mirrors the clone detector's exemption.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.test.ts", CELL_TO_STRING);
        let b = write(&dir, "b.test.ts", CELL_TO_STRING);
        assert!(run(&[&a, &b]).is_empty(), "sibling test specs are exempt");
    }

    #[test]
    fn helper_duplicated_in_spec_is_flagged() {
        // Shared test infrastructure (`render.tsx`) is NOT a spec: a function
        // pasted from it into a spec should be imported, so it stays flagged.
        // (saurenya: `Wrapper` in `test-helpers/render.tsx` vs a `.test.tsx`.)
        let dir = tempfile::tempdir().unwrap();
        let helper = write(&dir, "render.tsx", CELL_TO_STRING);
        let spec = write(&dir, "feature.test.tsx", CELL_TO_STRING);
        let diags = run(&[&helper, &spec]);
        assert_eq!(diags.len(), 1, "a helper duplicated into a spec is still flagged");
        // Canonical is the lexicographically-first path (`feature.test.tsx`);
        // the helper reports it.
        assert!(diags[0].path.ends_with("render.tsx"));
    }

    #[test]
    fn formatting_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let reformatted = CELL_TO_STRING.replace("  ", "\t").replace(") return", ")\n    return");
        let b = write(&dir, "b.ts", &reformatted);
        assert_eq!(run(&[&a, &b]).len(), 1, "token-based match ignores formatting");
    }

    #[test]
    fn overload_signatures_not_flagged() {
        // The bodiless overload signatures are skipped (no `body` field); only the
        // implementation has a body, and the two implementations differ.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            "function pick(x: number): number;\nfunction pick(x: string): string;\nfunction pick(x: unknown) { return x as number; }\n",
        );
        let b = write(
            &dir,
            "b.ts",
            "function pick(x: number): number;\nfunction pick(x: string): string;\nfunction pick(x: unknown) { return String(x); }\n",
        );
        assert!(run(&[&a, &b]).is_empty(), "overload signatures have no body and differ in impl");
    }

    #[test]
    fn bucket_of_three_yields_two_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let b = write(&dir, "b.ts", CELL_TO_STRING);
        let c = write(&dir, "c.ts", CELL_TO_STRING);
        let diags = run(&[&a, &b, &c]);
        assert_eq!(diags.len(), 2, "N files yield N-1 diagnostics");
        assert!(diags.iter().all(|d| d.message.contains("a.ts")));
        assert!(diags[0].path.ends_with("b.ts"));
        assert!(diags[1].path.ends_with("c.ts"));
    }

    #[test]
    fn same_name_same_body_different_param_type_not_flagged() {
        // Issue #4574 / recommended fix: two `toOption` map `{id,name}` to a
        // labelled option with byte-identical bodies but different parameter and
        // return types. Not interchangeable — different keys, no finding.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            "function toOption(e: ScopeEntity): Option {\n  return { value: e.id, label: e.name, extra: e.id };\n}\n",
        );
        let b = write(
            &dir,
            "b.ts",
            "function toOption(e: DataTableFilterEntity): MultipleSelectorOption {\n  return { value: e.id, label: e.name, extra: e.id };\n}\n",
        );
        assert!(
            run(&[&a, &b]).is_empty(),
            "different param/return types are not duplicates"
        );
    }

    #[test]
    fn same_name_same_body_different_arity_not_flagged() {
        // Issue #4574 shape 1: `wrap()` vs `wrap(queryClient)` — different arity,
        // same JSX-returning body shape. Distinct signatures, no finding.
        let dir = tempfile::tempdir().unwrap();
        let with_param = "\
function wrap(queryClient: QueryClient) {
  const factory = queryClient;
  return ({ children }: { children: ReactNode }) => factory;
}
";
        let no_param = "\
function wrap() {
  const factory = new QueryClient();
  return ({ children }: { children: ReactNode }) => factory;
}
";
        let a = write(&dir, "a.tsx", with_param);
        let b = write(&dir, "b.tsx", no_param);
        assert!(
            run(&[&a, &b]).is_empty(),
            "different arity functions are not duplicates"
        );
    }

    #[test]
    fn same_name_same_body_different_return_type_not_flagged() {
        // Issue #4574 shape 2 generalized: identical body and param name, but the
        // return-type annotation diverges. Distinct keys, no finding.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            "function load(id: string): Promise<User> {\n  const url = `/api/v1/users/${id}`;\n  return fetch(url).then((r) => r.json());\n}\n",
        );
        let b = write(
            &dir,
            "b.ts",
            "function load(id: string): Promise<Team> {\n  const url = `/api/v1/users/${id}`;\n  return fetch(url).then((r) => r.json());\n}\n",
        );
        assert!(
            run(&[&a, &b]).is_empty(),
            "different return-type annotations are not duplicates"
        );
    }

    #[test]
    fn same_name_same_body_different_generic_constraints_not_flagged() {
        // Issue #6827 (drizzle-orm): two dialect-specific `alias` functions with
        // byte-identical bodies, parameters, and return types but divergent
        // generic constraints (`TTable extends SQLiteTable | SQLiteViewBase` vs
        // `TTable extends GelTable | GelViewBase`). The constraints are enforced
        // at call sites, so the functions are not interchangeable. Distinct keys,
        // no finding.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "sqlite-core/alias.ts",
            "export function alias<TTable extends SQLiteTable | SQLiteViewBase, TAlias extends string>(\n  table: TTable,\n  alias: TAlias,\n): BuildAliasTable<TTable, TAlias> {\n  return new Proxy(table, new TableAliasProxyHandler(alias, false)) as any;\n}\n",
        );
        let b = write(
            &dir,
            "gel-core/alias.ts",
            "export function alias<TTable extends GelTable | GelViewBase, TAlias extends string>(\n  table: TTable,\n  alias: TAlias,\n): BuildAliasTable<TTable, TAlias> {\n  return new Proxy(table, new TableAliasProxyHandler(alias, false)) as any;\n}\n",
        );
        assert!(
            run(&[&a, &b]).is_empty(),
            "different generic constraints are not duplicates"
        );
    }

    #[test]
    fn same_name_same_generic_constraints_same_body_still_flagged() {
        // Acceptance: identical generic constraints, signature, and body across
        // files is a genuine copy-paste and is still flagged. Including the
        // generic constraints in the fingerprint only splits buckets that differ
        // in generics; it must not stop flagging real duplicates.
        let dir = tempfile::tempdir().unwrap();
        let f = "export function alias<TTable extends SQLiteTable | SQLiteViewBase, TAlias extends string>(\n  table: TTable,\n  alias: TAlias,\n): BuildAliasTable<TTable, TAlias> {\n  return new Proxy(table, new TableAliasProxyHandler(alias, false)) as any;\n}\n";
        let a = write(&dir, "a/alias.ts", f);
        let b = write(&dir, "b/alias.ts", f);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical generics + signature + body is still a duplicate");
        assert!(diags[0].message.contains("`alias`"));
    }

    #[test]
    fn same_name_same_signature_same_body_still_flagged() {
        // Acceptance: identical name, signature, and body across files is a
        // genuine copy-paste and is still flagged.
        let dir = tempfile::tempdir().unwrap();
        let f = "\
function toOption(e: ScopeEntity): Option {
  const value = e.id;
  const label = e.name;
  return { value, label };
}
";
        let a = write(&dir, "a.ts", f);
        let b = write(&dir, "b.ts", f);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical signature + body is still a duplicate");
        assert!(diags[0].message.contains("`toOption`"));
    }

    #[test]
    fn relaxed_dir_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "samples/a.ts", CELL_TO_STRING);
        let b = write(&dir, "samples/b.ts", CELL_TO_STRING);
        assert!(run(&[&a, &b]).is_empty(), "fixture/sample dirs are exempt");
    }

    #[test]
    fn fuzz_harness_dir_not_flagged() {
        // Issue #7060: a fuzz harness deliberately copies the function under test.
        // The fuzz dir is dropped from the corpus, so the src-side function — whose
        // only partner is the fuzz copy — is not flagged.
        let dir = tempfile::tempdir().unwrap();
        let src = write(&dir, "src/util.ts", CELL_TO_STRING);
        let fuzz = write(&dir, "fuzz/fuzz_targets/harness.ts", CELL_TO_STRING);
        assert!(run(&[&src, &fuzz]).is_empty(), "fuzz-harness function copies are exempt");
    }

    #[test]
    fn generated_content_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let gen_src = format!("// @generated\n{CELL_TO_STRING}");
        let a = write(&dir, "a.ts", &gen_src);
        let b = write(&dir, "b.ts", &gen_src);
        assert!(run(&[&a, &b]).is_empty(), "@generated files are exempt");
    }

    #[test]
    fn rust_fn_copied_across_crates_is_flagged() {
        // Issue #8476: this `pool()` sat in 27 test crates.
        let dir = tempfile::tempdir().unwrap();
        let pool = "\
async fn pool() -> PgPool {
    let url = std::env::var(\"DATABASE_URL\").expect(\"DATABASE_URL set for integration tests\");
    let pool = PgPool::connect(&url).await.expect(\"connect\");
    sqlx::migrate!(\"../../migrations\")
        .run(&pool)
        .await
        .expect(\"migrate\");
    pool
}
";
        write(&dir, "libs/a/Cargo.toml", "");
        write(&dir, "libs/b/Cargo.toml", "");
        let a = write(&dir, "libs/a/tests/db.rs", pool);
        let b = write(&dir, "libs/b/tests/db.rs", pool);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(*diags[0].path, *b.path);
        assert!(diags[0].message.starts_with("Duplicate function `pool`"));
        assert!(diags[0].message.contains("libs/a/tests/db.rs"));
    }

    #[test]
    fn rust_impl_method_copy_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let src = "\
impl Store {
    fn load(&self, key: &str) -> Option<String> {
        let guard = self.map.lock().unwrap();
        let value = guard.get(key).cloned();
        value.filter(|v| !v.is_empty())
    }
}
";
        let a = write(&dir, "a.rs", src);
        let b = write(&dir, "b.rs", src);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(*diags[0].path, *b.path);
    }

    #[test]
    fn rust_same_method_in_impls_of_different_types_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.rs",
            "\
impl fmt::Display for A {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, \"{}-{}\", self.id, self.name)
    }
}
",
        );
        let b = write(
            &dir,
            "b.rs",
            "\
impl fmt::Display for B {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, \"{}-{}\", self.id, self.name)
    }
}
",
        );
        assert!(run(&[&a, &b]).is_empty());
    }

    const USE_CRATE_CALLER: &str = "\
use crate::db::helper;

fn run_query(id: u32) -> u32 {
    let first = helper(id);
    let second = helper(first + 1);
    first + second + id
}
";

    #[test]
    fn rust_use_crate_callee_across_crates_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "libs/a/Cargo.toml", "");
        write(&dir, "libs/b/Cargo.toml", "");
        let a = write(&dir, "libs/a/src/q.rs", USE_CRATE_CALLER);
        let b = write(&dir, "libs/b/src/q.rs", USE_CRATE_CALLER);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn rust_inline_crate_path_across_crates_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "libs/a/Cargo.toml", "");
        write(&dir, "libs/b/Cargo.toml", "");
        let caller = "\
fn run_query(id: u32) -> u32 {
    let first = crate::db::helper(id);
    let second = crate::db::helper(first + 1);
    first + second + id
}
";
        let a = write(&dir, "libs/a/src/q.rs", caller);
        let b = write(&dir, "libs/b/src/q.rs", caller);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn rust_crate_struct_literal_across_crates_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "libs/a/Cargo.toml", "");
        write(&dir, "libs/b/Cargo.toml", "");
        let caller = "\
fn make(id: u32) -> u32 {
    let cfg = crate::Config { id, retries: 3 };
    cfg.id + cfg.retries + id
}
";
        let a = write(&dir, "libs/a/src/q.rs", caller);
        let b = write(&dir, "libs/b/src/q.rs", caller);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn rust_crate_typed_param_keeps_body_guard() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "libs/a/Cargo.toml", "");
        write(&dir, "libs/b/Cargo.toml", "");
        let caller = "\
fn run_query(p: &crate::db::Pool, id: u32) -> u32 {
    let first = crate::db::helper(p, id);
    first + id + 1
}
";
        let a = write(&dir, "libs/a/src/q.rs", caller);
        let b = write(&dir, "libs/b/src/q.rs", caller);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn rust_use_crate_callee_within_one_crate_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "libs/a/Cargo.toml", "");
        let a = write(&dir, "libs/a/src/q1.rs", USE_CRATE_CALLER);
        let b = write(&dir, "libs/a/src/q2.rs", USE_CRATE_CALLER);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn rust_copy_without_manifest_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let src = "\
fn total(items: &[u32]) -> u32 {
    let mut sum = 0;
    for item in items {
        sum += item * 2;
    }
    sum
}
";
        let a = write(&dir, "a.rs", src);
        let b = write(&dir, "b.rs", src);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(*diags[0].path, *b.path);
    }

    #[test]
    fn rust_divergent_use_path_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let body = "\
fn open(url: &str) -> Conn {
    let conn = connect(url);
    conn.set_timeout(30);
    conn
}
";
        let a = write(&dir, "a.rs", &format!("use a::connect;\n\n{body}"));
        let b = write(&dir, "b.rs", &format!("use b::connect;\n\n{body}"));
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn rust_divergent_private_callee_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "libs/a/Cargo.toml", "");
        write(&dir, "libs/b/Cargo.toml", "");
        let caller = "\
fn compute(id: u32) -> u32 {
    let first = helper();
    let second = helper() + id;
    first + second + id
}
";
        let a = write(
            &dir,
            "libs/a/src/c.rs",
            &format!("{caller}\nfn helper() -> u32 {{\n    1\n}}\n"),
        );
        let b = write(
            &dir,
            "libs/b/src/c.rs",
            &format!("{caller}\nfn helper() -> u32 {{\n    2\n}}\n"),
        );
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn rust_renamed_copy_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.rs",
            "\
fn load_rows(limit: usize) -> Vec<u32> {
    let rows = fetch_all();
    rows.into_iter().take(limit).collect()
}
",
        );
        let b = write(
            &dir,
            "b.rs",
            "\
fn fetch_rows(max: usize) -> Vec<u32> {
    let items = fetch_all();
    items.into_iter().take(max).collect()
}
",
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("renamed copy"));
        assert!(diags[0].message.contains("load_rows"));
    }

    #[test]
    fn rust_renamed_nested_fn_copy_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.rs",
            "\
fn total(xs: &[u32]) -> u32 {
    fn step(x: u32) -> u32 { x + 1 }
    xs.iter().map(|x| step(*x)).sum()
}
",
        );
        let b = write(
            &dir,
            "b.rs",
            "\
fn sum_all(xs: &[u32]) -> u32 {
    fn bump(x: u32) -> u32 { x + 1 }
    xs.iter().map(|x| bump(*x)).sum()
}
",
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("renamed copy"));
    }

    #[test]
    fn minified_bundle_not_flagged() {
        // Issue #5114: jsrsasign ships `*-min.js` bundles that inline every source
        // function. A duplicate where one side lives in a `-min.js` bundle is a
        // build artifact, not authored duplication, so it must not be reported.
        let dir = tempfile::tempdir().unwrap();
        let src = write(&dir, "src/util.js", CELL_TO_STRING);
        let bundle = write(&dir, "jsrsasign-all-min.js", CELL_TO_STRING);
        assert!(
            run(&[&src, &bundle]).is_empty(),
            "a duplicate involving a -min.js bundle is exempt"
        );
    }

    #[test]
    fn dot_min_bundle_not_flagged() {
        // The canonical `.min.js` naming is equally a build artifact.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "min/base64x-1.1.min.js", CELL_TO_STRING);
        let b = write(&dir, "npm/lib/base64x-1.1.min.js", CELL_TO_STRING);
        assert!(run(&[&a, &b]).is_empty(), "two .min.js bundles are exempt");
    }

    #[test]
    fn webpack_bundle_not_flagged() {
        // Webpack-style `*.bundle.js` artifacts are equally machine-emitted.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "dist/app.bundle.js", CELL_TO_STRING);
        let b = write(&dir, "src/util.js", CELL_TO_STRING);
        assert!(run(&[&a, &b]).is_empty(), "a .bundle.js artifact is exempt");
    }

    #[test]
    fn real_source_still_flagged_alongside_min_in_name() {
        // A real source file whose name merely contains `min` (e.g. `admin.js`)
        // must stay linted: the duplicate across two authored files is reported.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "src/admin.js", CELL_TO_STRING);
        let b = write(&dir, "src/reader.js", CELL_TO_STRING);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicates across real source files are still flagged");
        assert!(diags[0].message.contains("`cellToString`"));
    }

    #[test]
    fn single_file_is_noop() {
        let f = SourceFile {
            path: PathBuf::from("/tmp/only.ts"),
            language: Language::TypeScript,
        };
        assert!(run(&[&f]).is_empty());
    }

    #[test]
    fn private_helper_across_packages_not_flagged() {
        // Issue #5777 (tonaljs/tonal): a file-private helper (`ascR`) is copied
        // into two separate published npm packages to stay self-contained and
        // avoid a cross-package dependency cycle. There is no importable surface
        // to share, so it must not be flagged.
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/collection", "@tonaljs/collection");
        write_pkg(&dir, "packages/array", "@tonaljs/array");
        let a = write(&dir, "packages/collection/index.ts", PRIVATE_HELPER);
        let b = write(&dir, "packages/array/index.ts", PRIVATE_HELPER);
        assert!(
            run(&[&a, &b]).is_empty(),
            "a file-private helper copied across npm packages is exempt"
        );
    }

    #[test]
    fn private_helper_same_package_still_flagged() {
        // Within one package a file-private helper is trivially extractable to a
        // local module and imported, so the duplicate is still a smell.
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/array", "@tonaljs/array");
        let a = write(&dir, "packages/array/asc.ts", PRIVATE_HELPER);
        let b = write(&dir, "packages/array/range.ts", PRIVATE_HELPER);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "intra-package private duplicates are still flagged");
        assert!(diags[0].message.contains("`ascR`"));
    }

    #[test]
    fn exported_function_across_packages_still_flagged() {
        // An *exported* function duplicated across packages is genuinely
        // hoistable into a shared package, so cross-package alone does not exempt
        // it — the export gives an importable surface.
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/collection", "@tonaljs/collection");
        write_pkg(&dir, "packages/array", "@tonaljs/array");
        let exported = format!("export {PRIVATE_HELPER}");
        let a = write(&dir, "packages/collection/index.ts", &exported);
        let b = write(&dir, "packages/array/index.ts", &exported);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "exported cross-package duplicates are still flagged");
        assert!(diags[0].message.contains("`ascR`"));
    }

    // A platform entry's `serve`, parameterized by the runtime package it calls
    // through the `srvxServe` alias.
    fn serve_entry(specifier: &str) -> String {
        format!(
            "import {{ serve as srvxServe }} from \"{specifier}\";\n\
             export function serve(app: H3, options?: ServerOptions): Server {{\n\
            \x20 freezeApp(app);\n\
            \x20 return srvxServe({{ fetch: app.fetch, ...options }});\n\
             }}\n"
        )
    }

    #[test]
    fn divergent_import_specifier_not_flagged() {
        // Issue #6394 (unjs/h3): `h3/src/_entries/{bun,node}.ts` carry
        // byte-identical `serve` bodies, each calling `srvxServe` aliased from a
        // different runtime package (`srvx/bun` vs `srvx/node`). The platform is
        // encoded in the import path, not a parameter, so they cannot be hoisted
        // to one shared module — not a true duplicate.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "bun.ts", &serve_entry("srvx/bun"));
        let b = write(&dir, "node.ts", &serve_entry("srvx/node"));
        assert!(
            run(&[&a, &b]).is_empty(),
            "identical bodies whose alias resolves to different packages are exempt"
        );
    }

    #[test]
    fn same_import_specifier_still_flagged() {
        // Negative control: when the same-named alias resolves to the *same*
        // package in both files, there is no divergence and the copy-paste is a
        // genuine duplicate — still flagged.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", &serve_entry("srvx/node"));
        let b = write(&dir, "b.ts", &serve_entry("srvx/node"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "same import specifier is still a duplicate");
        assert!(diags[0].message.contains("`serve`"));
    }

    #[test]
    fn pure_body_without_imports_still_flagged() {
        // Negative control: a hoistable pure helper referencing no imports (only
        // params, locals, and globals) is a genuine duplicate. This proves the
        // exemption is keyed on import-source divergence, not a blanket pass.
        let pure = "\
export function shallowEqual(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}
";
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", pure);
        let b = write(&dir, "b.ts", pure);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "an import-free pure duplicate is still flagged");
        assert!(diags[0].message.contains("`shallowEqual`"));
    }

    // A framework-adapter package's thin public hook: an exported `useMachine`
    // delegating to a package-local `useActor` imported from `specifier`.
    fn use_machine_entry(specifier: &str) -> String {
        format!(
            "import {{ useActor }} from \"{specifier}\";\n\
             export function useMachine(machine: AnyStateMachine, ...opts: unknown[]): Actor {{\n\
            \x20 const actor = useActor(machine, opts);\n\
            \x20 return actor;\n\
             }}\n"
        )
    }

    #[test]
    fn relative_import_delegation_across_packages_not_flagged() {
        // Issue #6277 (statelyai/xstate): framework-adapter packages
        // (`@xstate/svelte`, `@xstate/solid`) each export an identical thin
        // `useMachine` whose body delegates to a package-LOCAL `useActor`
        // imported via a relative specifier. The same `./useActor` text resolves
        // to a different framework-specific file per package, so the bodies
        // cannot be hoisted into one shared module — not a true duplicate.
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/xstate-svelte", "@xstate/svelte");
        write_pkg(&dir, "packages/xstate-solid", "@xstate/solid");
        let a = write(
            &dir,
            "packages/xstate-svelte/src/useMachine.ts",
            &use_machine_entry("./useActor"),
        );
        let b = write(
            &dir,
            "packages/xstate-solid/src/useMachine.ts",
            &use_machine_entry("./useActor"),
        );
        assert!(
            run(&[&a, &b]).is_empty(),
            "exported hooks delegating to a package-local relative import are exempt"
        );
    }

    #[test]
    fn pure_export_across_packages_still_flagged() {
        // Negative control for #6277: a pure exported helper (`shallowEqual`) with
        // no imports is genuinely hoistable to a shared package, so duplicating it
        // across packages stays a smell — the exemption is keyed on delegation to
        // a package-local relative import, not on the package boundary alone.
        let pure = "\
export function shallowEqual(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}
";
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/xstate-react", "@xstate/react");
        write_pkg(&dir, "packages/xstate-store", "@xstate/store");
        let a = write(&dir, "packages/xstate-react/src/shallowEqual.ts", pure);
        let b = write(&dir, "packages/xstate-store/src/shallowEqual.ts", pure);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a pure cross-package duplicate is still flagged");
        assert!(diags[0].message.contains("`shallowEqual`"));
    }

    #[test]
    fn relative_import_delegation_same_package_still_flagged() {
        // Within one package the two relative `./useActor` imports resolve to the
        // same file, so the delegating hook is trivially hoistable to a local
        // module and the duplicate is still a smell.
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/xstate-svelte", "@xstate/svelte");
        let a = write(
            &dir,
            "packages/xstate-svelte/src/useMachine.ts",
            &use_machine_entry("./useActor"),
        );
        let b = write(
            &dir,
            "packages/xstate-svelte/src/useMachineAlias.ts",
            &use_machine_entry("./useActor"),
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "intra-package relative-delegating duplicates are still flagged");
        assert!(diags[0].message.contains("`useMachine`"));
    }

    #[test]
    fn bare_import_delegation_across_packages_still_flagged() {
        // Negative control for #6277: a shared body free-var imported from a
        // bare/external package resolves to the same module in both packages — no
        // package-local divergence — so the cross-package duplicate is genuinely
        // hoistable and still flags.
        let dir = tempfile::tempdir().unwrap();
        write_pkg(&dir, "packages/a", "@scope/a");
        write_pkg(&dir, "packages/b", "@scope/b");
        let a = write(&dir, "packages/a/src/useMachine.ts", &use_machine_entry("xstate"));
        let b = write(&dir, "packages/b/src/useMachine.ts", &use_machine_entry("xstate"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a bare-import cross-package duplicate is still flagged");
        assert!(diags[0].message.contains("`useMachine`"));
    }

    // An adapter error module mirroring prisma's `adapter-neon` /
    // `adapter-better-sqlite3`: a byte-identical exported `convertDriverError`
    // delegating to module-local `isDriverError`/`mapDriverError` whose bodies map
    // driver-specific error codes and so differ per adapter.
    fn adapter_errors(guard: &str, map_arm: &str) -> String {
        format!(
            "function isDriverError(error: unknown): boolean {{\n\
            \x20 return {guard};\n\
             }}\n\
             function mapDriverError(error: DriverError): MappedError {{\n\
            \x20 switch (error.code) {{\n\
            \x20   {map_arm}\n\
            \x20   default:\n\
            \x20     return {{ kind: \"Unknown\" }};\n\
            \x20 }}\n\
             }}\n\
             export function convertDriverError(error: unknown): DriverAdapterErrorObject {{\n\
            \x20 if (isDriverError(error)) {{\n\
            \x20   return {{ originalCode: error.code, originalMessage: error.message, ...mapDriverError(error) }};\n\
            \x20 }}\n\
            \x20 throw error;\n\
             }}\n"
        )
    }

    #[test]
    fn module_local_divergent_delegate_not_flagged() {
        // Issue #6902 (prisma/prisma): `adapter-neon` and `adapter-better-sqlite3`
        // each export a byte-identical `convertDriverError` whose body delegates to
        // a module-local `isDriverError`/`mapDriverError` defined differently per
        // file (PostgreSQL vs SQLite error mapping). Hoisting `convertDriverError`
        // into a shared module would rebind the callees to the shared versions and
        // break both adapters — not a true duplicate.
        let dir = tempfile::tempdir().unwrap();
        let neon = adapter_errors(
            "typeof error === \"object\" && error !== null && \"severity\" in error",
            "case \"22001\":\n      return { kind: \"LengthMismatch\", column: error.column };",
        );
        let sqlite = adapter_errors(
            "error instanceof Error && error.name === \"SqliteError\"",
            "case \"SQLITE_BUSY\":\n      return { kind: \"SocketTimeout\" };",
        );
        let a = write(&dir, "adapter-neon/src/errors.ts", &neon);
        let b = write(&dir, "adapter-better-sqlite3/src/errors.ts", &sqlite);
        assert!(
            run(&[&a, &b]).is_empty(),
            "a body delegating to divergent module-local callees is exempt"
        );
    }

    #[test]
    fn shared_imported_delegate_still_flagged() {
        // Negative control for #6902: the same delegating `convertDriverError`, but
        // `isDriverError`/`mapDriverError` are *imported* from a shared module under
        // the same specifier in both files. The callees resolve to one shared
        // implementation, so the function is genuinely hoistable and stays flagged —
        // proving the exemption is keyed on module-local divergence, not on
        // delegation alone.
        let dir = tempfile::tempdir().unwrap();
        let f = "import { isDriverError, mapDriverError } from \"./shared\";\n\
                 export function convertDriverError(error: unknown): DriverAdapterErrorObject {\n\
                \x20 if (isDriverError(error)) {\n\
                \x20   return { originalCode: error.code, originalMessage: error.message, ...mapDriverError(error) };\n\
                \x20 }\n\
                \x20 throw error;\n\
                 }\n";
        let a = write(&dir, "a.ts", f);
        let b = write(&dir, "b.ts", f);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "an imported shared delegate stays a duplicate");
        assert!(diags[0].message.contains("`convertDriverError`"));
    }

    #[test]
    fn recursive_private_helper_still_flagged() {
        // A recursive file-private helper copied verbatim references its own name,
        // which is itself a top-level module-local declaration. The self-reference
        // resolves to the same duplicated body in both files, so it is not a
        // divergence: the own name is excluded and the duplicate stays flagged.
        let dir = tempfile::tempdir().unwrap();
        let f = "\
function deepClone(value: unknown): unknown {
  if (Array.isArray(value)) return value.map((v) => deepClone(v));
  if (value && typeof value === \"object\") {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value)) out[key] = deepClone(value[key]);
    return out;
  }
  return value;
}
";
        let a = write(&dir, "a.ts", f);
        let b = write(&dir, "b.ts", f);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a recursive private helper is still a duplicate");
        assert!(diags[0].message.contains("`deepClone`"));
    }

    #[test]
    fn recursive_exported_vs_private_still_flagged() {
        // Locks the own-name exclusion on its load-bearing path: export status is
        // not part of the bucket key, so a recursive function exported in one file
        // and private in the other still buckets together. The private side lists
        // its own name in `module_local_decls` while the exported side does not, so
        // without excluding the shared name the self-reference would read as a
        // one-sided divergence and wrongly exempt this genuine duplicate.
        let dir = tempfile::tempdir().unwrap();
        let body = "\
function factorial(n: number): number {
  if (n <= 1) return 1;
  return n * factorial(n - 1);
}
";
        let a = write(&dir, "a.ts", &format!("export {body}"));
        let b = write(&dir, "b.ts", body);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a recursive duplicate exported in one file is still flagged");
        assert!(diags[0].message.contains("`factorial`"));
    }

    #[test]
    fn const_arrow_divergent_module_local_not_flagged() {
        // The divergent module-local callee may itself be an arrow/`const`
        // binding, not only a `function` declaration. Two byte-identical exported
        // `convertDriverError`s each delegate to a `const mapDriverError = (…) =>`
        // whose body differs per file, so they are not interchangeable.
        let dir = tempfile::tempdir().unwrap();
        let module = |arm: &str| {
            format!(
                "const mapDriverError = (error: DriverError): MappedError => {{\n\
                \x20 switch (error.code) {{\n\
                \x20   {arm}\n\
                \x20   default:\n\
                \x20     return {{ kind: \"Unknown\" }};\n\
                \x20 }}\n\
                 }};\n\
                 export function convertDriverError(error: DriverError): MappedError {{\n\
                \x20 const mapped = mapDriverError(error);\n\
                \x20 return {{ ...mapped, originalCode: error.code }};\n\
                 }}\n"
            )
        };
        let a = write(&dir, "neon.ts", &module("case \"22001\":\n      return { kind: \"LengthMismatch\" };"));
        let b = write(&dir, "sqlite.ts", &module("case \"SQLITE_BUSY\":\n      return { kind: \"SocketTimeout\" };"));
        assert!(
            run(&[&a, &b]).is_empty(),
            "a divergent const-arrow module-local callee is exempt"
        );
    }

    #[test]
    fn identical_module_local_delegate_still_flagged() {
        // Negative control: when the module-local callee is byte-identical in both
        // files, the wrapper is genuinely hoistable alongside it, so both the
        // wrapper and the shared helper stay flagged. Proves the exemption keys on
        // the callee actually *diverging*, not on delegation to a module-local
        // name per se.
        let dir = tempfile::tempdir().unwrap();
        let f = "\
function helper(value: number): number {
  const scaled = value * 2;
  return scaled + 1;
}
export function compute(input: number): number {
  const base = helper(input);
  return base + helper(base);
}
";
        let a = write(&dir, "a.ts", f);
        let b = write(&dir, "b.ts", f);
        let diags = run(&[&a, &b]);
        // Both the wrapper `compute` and the shared `helper` are real duplicates.
        let names: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(
            names.iter().any(|m| m.contains("`compute`")),
            "the wrapper delegating to an identical module-local helper is still flagged"
        );
        assert!(
            names.iter().any(|m| m.contains("`helper`")),
            "the identical module-local helper is still flagged"
        );
    }

    // A Vue component whose `setup(props)` declares a nested `handleKeydown`
    // capturing `props` and a `const { onStackTop } = useStack(…, zarg)`. The
    // `useStack` z-index argument (`zarg`) varies per component while
    // `handleKeydown`'s body is byte-identical.
    fn keydown_component(zarg: &str) -> String {
        format!(
            "import {{ defineComponent }} from \"vue\";\n\
             import {{ call, useStack }} from \"../shared\";\n\
             const zIndex = 100;\n\
             const normalizedZIndex = 200;\n\
             export default defineComponent({{\n\
            \x20 setup(props) {{\n\
            \x20   const {{ onStackTop }} = useStack(() => props.show, {zarg});\n\
            \x20   function handleKeydown(event: KeyboardEvent) {{\n\
            \x20     if (!onStackTop() || event.key !== \"Escape\" || !props.show) return;\n\
            \x20     call(props.onKeyEscape);\n\
            \x20     if (!props.closeOnKeyEscape) return;\n\
            \x20     call(props[\"onUpdate:show\"], false);\n\
            \x20   }}\n\
            \x20   return {{ handleKeydown }};\n\
            \x20 }},\n\
             }});\n"
        )
    }

    #[test]
    fn nested_closure_capturing_enclosing_scope_not_flagged() {
        // Issue #7338 (varletjs/varlet): `Popup.tsx` and `Overlay.tsx` each nest a
        // byte-identical `handleKeydown` inside their `setup(props)`. The body
        // captures `props` (the setup parameter) and `onStackTop` (a `const`
        // destructured from a per-component `useStack(…)` call, built from
        // `normalizedZIndex` in one file and `zIndex` in the other). The captures
        // differ per component, so the closures are not interchangeable and cannot
        // be hoisted into a shared module without changing their signature — not a
        // true duplicate.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "overlay/Overlay.tsx", &keydown_component("zIndex"));
        let b = write(&dir, "popup/Popup.tsx", &keydown_component("normalizedZIndex"));
        assert!(
            run(&[&a, &b]).is_empty(),
            "a nested closure capturing enclosing-scope bindings is exempt"
        );
    }

    #[test]
    fn module_scope_no_capture_helper_still_flagged() {
        // Negative control for #7338: a byte-identical *module-scope* helper with
        // no enclosing-scope captures (only its own parameters and locals) is
        // genuinely hoistable, so the copy-paste stays a smell. Proves the
        // exemption keys on the nested-and-captures signal, not on any nesting or
        // free-variable presence alone.
        let helper = "\
function helper(a: number, b: number): number {
  const sum = a + b;
  const scaled = sum * 2;
  return scaled - a;
}
";
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", helper);
        let b = write(&dir, "b.ts", helper);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a module-scope no-capture duplicate is still flagged");
        assert!(diags[0].message.contains("`helper`"));
    }
}
