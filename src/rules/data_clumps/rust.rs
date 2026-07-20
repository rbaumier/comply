//! data-clumps Rust backend — flag structs sharing 3+ identical field names.
//!
//! Walks the AST to find `struct_item` nodes, extracts their field names,
//! and flags when the same 3-field subset appears in 2+ structs.
//!
//! Borrowed "view" structs (a lifetime parameter plus at least one
//! reference-typed field) are excluded: they intentionally mirror an owned
//! struct's field names but cannot be merged with it.
//!
//! A shared subset whose every field is typed solely by the host struct's own
//! declared generic type parameters (e.g. `g: G`, `init: Init`,
//! `r: PhantomData<R>`) is also excluded: extracting it yields a struct that
//! must re-declare the same parameters, so no duplication is removed.
//!
//! Fields sharing a name but disagreeing on optionality (`Option<T>` in one
//! struct, bare `T` in another) also do not count toward a clump: no common
//! type can hold both without dropping the mandatory side's all-present
//! invariant or wrapping the optional side in a pointless `Option`, so there is
//! nothing to extract.
//!
//! Strong/weak ownership-pair structs are excluded as well: when every shared
//! field is `Arc<X>`/`Rc<X>` in one struct and `Weak<X>` in the other (same
//! inner `X`), the two are a deliberate strong/weak counterpart pair (the
//! `Weak` struct mirrors the owner's field names so `upgrade()` reconstructs
//! the strong form). They cannot be merged — the wrapper changes from a strong
//! to a weak handle — so they do not form a data clump.
//!
//! Structs carrying a layout-constraining `repr` attribute (`#[repr(C)]`,
//! `#[repr(packed)]`, `#[repr(transparent)]`, `#[repr(align(N))]`, or any
//! combination such as `#[repr(C, packed)]`) are excluded: these pin an exact
//! in-memory layout for FFI or byte-level casts (e.g. `bytemuck`), so factoring
//! shared fields into a nested type would change the layout and break the
//! contract.
//!
//! Same-name `struct_item`s that each carry a `#[cfg(...)]` conditional-
//! compilation gate are collapsed to a single representative before the
//! pairwise scan. Two definitions of one struct name under mutually-exclusive
//! gates (a `#[cfg(feature = "v1")]` / `#[cfg(feature = "v2")]` versioned
//! redefinition) are never present in the same build, so their shared field
//! subset does not "appear together" in two coexisting structs and there is
//! nothing to extract; counting both would double-count one logical type.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::{FxHashMap, FxHashSet};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir {
        return;
    }

    let mut struct_fields: Vec<StructFields> = Vec::new();
    collect_structs(node, source, &mut struct_fields);
    dedup_cfg_twins(&mut struct_fields);

    let ptrs_by_line: FxHashMap<usize, &FxHashMap<String, (Strength, String)>> = struct_fields
        .iter()
        .map(|sf| (sf.line, &sf.smart_ptr_fields))
        .collect();

    // For each 3-field subset, record every struct that contains it, noting
    // whether that struct types the subset entirely with its own declared
    // generic parameters (in which case extraction removes no duplication).
    // Each subset field carries its optionality (`Option<T>` vs bare `T`) so a
    // shared field name groups two structs only when both agree on it: a field
    // that is optional in one struct and mandatory in the other cannot be
    // factored into one shared type.
    let mut subset_occurrences: FxHashMap<Vec<(String, bool)>, Vec<(usize, bool)>> =
        FxHashMap::default();
    for sf in &struct_fields {
        for combo in combinations(&sf.names, 3) {
            let all_generic = combo.iter().all(|f| sf.generic_param_only.contains(f));
            let keyed: Vec<(String, bool)> = combo
                .into_iter()
                .map(|f| {
                    let optional = sf.optional_fields.contains(&f);
                    (f, optional)
                })
                .collect();
            subset_occurrences
                .entry(keyed)
                .or_default()
                .push((sf.line, all_generic));
        }
    }

    let mut flagged_lines: FxHashSet<usize> = FxHashSet::default();
    let mut results: Vec<(usize, String)> = Vec::new();

    for (subset, occurrences) in &subset_occurrences {
        // A struct whose every subset field is one of its own generic
        // parameters cannot be merged into a shared type, so it does not count
        // toward the clump.
        let flaggable: Vec<usize> = occurrences
            .iter()
            .filter(|&&(_, all_generic)| !all_generic)
            .map(|&(line, _)| line)
            .collect();
        // A two-struct clash whose every shared field is `Arc<X>`/`Rc<X>` in one
        // and `Weak<X>` in the other (same inner `X`) is a strong/weak ownership
        // pair, not a data clump.
        if flaggable.len() == 2
            && let Some(&a) = ptrs_by_line.get(&flaggable[0])
            && let Some(&b) = ptrs_by_line.get(&flaggable[1])
            && is_strong_weak_pair(subset, a, b)
        {
            continue;
        }
        if flaggable.len() >= 2 {
            let field_names = subset
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            for &line in &flaggable {
                if flagged_lines.insert(line) {
                    results.push((
                        line,
                        format!(
                            "Fields [{}] appear together in {} structs \
                             \u{2014} extract into a shared type.",
                            field_names,
                            flaggable.len(),
                        ),
                    ));
                }
            }
        }
    }

    results.sort_by_key(|(line, _)| *line);
    for (line, message) in results {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: "data-clumps".into(),
            message,
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Per-struct field data gathered for clump detection.
struct StructFields {
    line: usize,
    /// The struct's declared type name. Same-name structs under divergent
    /// `#[cfg]` gates are one logical type re-declared per build variant.
    name: String,
    /// True if the struct carries a `#[cfg(...)]` conditional-compilation gate —
    /// a build variant. `#[cfg_attr(...)]` does not count: it conditionally
    /// applies an attribute but always compiles the item, so it cannot make two
    /// same-name definitions mutually exclusive. Same-name gated structs are
    /// versioned redefinitions collapsed to one before clump detection.
    cfg_gated: bool,
    names: Vec<String>,
    /// Field names whose type is determined solely by the struct's own declared
    /// generic type parameters.
    generic_param_only: FxHashSet<String>,
    /// Field names whose declared type is `Option<…>`. A field optional in one
    /// struct and mandatory in another cannot be merged into a common type, so
    /// it must not count toward a shared clump.
    optional_fields: FxHashSet<String>,
    /// For each field typed as a single `Arc`/`Rc`/`Weak` smart pointer, its
    /// strength and inner type text (`Weak<Mutex<S>>` → `(Weak, "Mutex<S>")`).
    /// Used to recognise strong/weak ownership-pair structs.
    smart_ptr_fields: FxHashMap<String, (Strength, String)>,
}

/// Strength of a reference-counted smart-pointer wrapper.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Strength {
    /// `Arc<X>` or `Rc<X>` — owns a strong reference.
    Strong,
    /// `Weak<X>` — a non-owning back-reference.
    Weak,
}

/// Recursively collect struct field sets from the AST.
fn collect_structs(node: tree_sitter::Node, source: &[u8], out: &mut Vec<StructFields>) {
    if node.kind() == "struct_item" {
        if crate::rules::rust_helpers::is_in_test_context(node, source) {
            return;
        }
        let declared = declared_type_param_names(node, source);
        // Look for field_declaration_list child.
        let mut names: Vec<String> = Vec::new();
        let mut generic_param_only: FxHashSet<String> = FxHashSet::default();
        let mut optional_fields: FxHashSet<String> = FxHashSet::default();
        let mut smart_ptr_fields: FxHashMap<String, (Strength, String)> = FxHashMap::default();
        let child_count = node.named_child_count();
        for i in 0..child_count {
            if let Some(child) = node.named_child(i)
                && child.kind() == "field_declaration_list"
            {
                let field_count = child.named_child_count();
                for j in 0..field_count {
                    if let Some(field) = child.named_child(j)
                        && field.kind() == "field_declaration"
                        && let Some(name_node) = field.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(source)
                    {
                        names.push(name.to_string());
                        if let Some(ty) = field.child_by_field_name("type") {
                            if type_is_generic_param_only(ty, &declared, source) {
                                generic_param_only.insert(name.to_string());
                            }
                            if type_is_option(ty, source) {
                                optional_fields.insert(name.to_string());
                            }
                            if let Some(ptr) = smart_pointer_parts(ty, source) {
                                smart_ptr_fields.insert(name.to_string(), ptr);
                            }
                        }
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        if names.len() >= 3
            && !is_borrowed_view_struct(node)
            && !has_layout_repr_attr(node, source)
        {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string();
            out.push(StructFields {
                line: node.start_position().row + 1,
                name,
                cfg_gated: has_cfg_gate(node, source),
                names,
                generic_param_only,
                optional_fields,
                smart_ptr_fields,
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_structs(cursor.node(), source, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Collapse conditional-compilation twins: `struct_item`s that share a name and
/// each carry a `#[cfg(...)]` gate are one logical type re-declared under
/// mutually-exclusive build variants (a `#[cfg(feature = "v1")]` /
/// `#[cfg(feature = "v2")]` versioned redefinition), never present together in
/// one compilation. Keep the first such definition per name and drop the rest,
/// so their shared field subset is not double-counted as a two-struct clump.
/// Ungated same-name structs (distinct types in separate modules) are left
/// untouched, and gated structs still clump against differently-named structs.
fn dedup_cfg_twins(structs: &mut Vec<StructFields>) {
    let mut kept_gated_names: FxHashSet<String> = FxHashSet::default();
    structs.retain(|sf| !sf.cfg_gated || kept_gated_names.insert(sf.name.clone()));
}

/// True if `struct_node` carries a `#[cfg(...)]` conditional-compilation gate as
/// a preceding `attribute_item` sibling. Only `cfg` counts, not `cfg_attr`:
/// `#[cfg_attr(...)]` conditionally applies an attribute but always compiles the
/// item, so it does not make two same-name definitions mutually exclusive.
/// Interleaved comment siblings are skipped and unrelated attributes
/// (`#[derive(...)]`) are traversed past. Keying on the `attribute`'s path child
/// — not a raw text scan — means an attribute merely ending in `cfg`, or the
/// token `cfg` in a comment, does not match.
fn has_cfg_gate(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = struct_node.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                let mut cursor = s.walk();
                let path = s
                    .children(&mut cursor)
                    .find(|c| c.kind() == "attribute")
                    .and_then(|attr| attr.named_child(0))
                    .and_then(|p| p.utf8_text(source).ok());
                if path == Some("cfg") {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True when `ty` is determined solely by the host struct's own declared
/// generic type parameters: a bare `type_identifier` that is one of `declared`,
/// or a `generic_type` (e.g. `PhantomData<R>`, `Option<G>`) whose
/// `type_arguments` are all `type_identifier`s in `declared`. The wrapper
/// constructor (`PhantomData`/`Option`/`Box`…) is ignored; only the type
/// arguments must be struct-declared parameters.
fn type_is_generic_param_only(ty: tree_sitter::Node, declared: &[&str], source: &[u8]) -> bool {
    match ty.kind() {
        "type_identifier" => ty.utf8_text(source).is_ok_and(|t| declared.contains(&t)),
        "generic_type" => {
            let Some(args) = ty.child_by_field_name("type_arguments") else {
                return false;
            };
            let mut cursor = args.walk();
            let mut saw_type_arg = false;
            for arg in args.named_children(&mut cursor) {
                match arg.kind() {
                    "type_identifier" => {
                        saw_type_arg = true;
                        if !arg.utf8_text(source).is_ok_and(|t| declared.contains(&t)) {
                            return false;
                        }
                    }
                    "lifetime" => {}
                    _ => return false,
                }
            }
            saw_type_arg
        }
        _ => false,
    }
}

/// True if `ty` is `Option<…>`: a `generic_type` whose constructor identifier
/// is `Option`. A field's optionality is a structural property of its type — a
/// shared field name that is `Option<T>` in one struct and a bare `T` in
/// another cannot be factored into a single shared type, so it must not count
/// toward a data clump.
fn type_is_option(ty: tree_sitter::Node, source: &[u8]) -> bool {
    ty.kind() == "generic_type"
        && ty
            .child_by_field_name("type")
            .and_then(|constructor| constructor.utf8_text(source).ok())
            == Some("Option")
}

/// Names of the `type_identifier` generic parameters declared on the struct's
/// `type_parameters` node (skipping lifetimes and const generics).
fn declared_type_param_names<'a>(struct_node: tree_sitter::Node, source: &'a [u8]) -> Vec<&'a str> {
    let Some(type_params) = struct_node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = type_params.walk();
    let mut names = Vec::new();
    for param in type_params.children(&mut cursor) {
        if param.kind() != "type_parameter" {
            continue;
        }
        if let Some(name_node) = param.child_by_field_name("name")
            && name_node.kind() == "type_identifier"
            && let Ok(text) = name_node.utf8_text(source)
        {
            names.push(text);
        }
    }
    names
}

/// True if `struct_node` is a borrowed "view" type: it has a lifetime
/// parameter and at least one reference-typed field (e.g. `RealmRef<'a>`
/// with `&'a str` fields, mirroring an owned `Realm`). Such a struct
/// intentionally shares its field names with the owned version but cannot
/// be merged with it, so it does not participate in data-clump detection.
fn is_borrowed_view_struct(struct_node: tree_sitter::Node) -> bool {
    has_lifetime_param(struct_node) && has_reference_field(struct_node)
}

fn has_lifetime_param(struct_node: tree_sitter::Node) -> bool {
    let Some(tp) = struct_node.child_by_field_name("type_parameters") else {
        return false;
    };
    let mut cursor = tp.walk();
    tp.named_children(&mut cursor)
        .any(|c| c.kind() == "lifetime_parameter")
}

fn has_reference_field(struct_node: tree_sitter::Node) -> bool {
    let child_count = struct_node.named_child_count();
    for i in 0..child_count {
        if let Some(list) = struct_node.named_child(i)
            && list.kind() == "field_declaration_list"
        {
            let field_count = list.named_child_count();
            for j in 0..field_count {
                if let Some(field) = list.named_child(j)
                    && field.kind() == "field_declaration"
                    && let Some(ty) = field.child_by_field_name("type")
                    && type_contains_reference(ty)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn type_contains_reference(node: tree_sitter::Node) -> bool {
    if node.kind() == "reference_type" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(type_contains_reference)
}

/// True if `struct_node` carries a layout-constraining `repr` attribute —
/// `#[repr(C)]`, `#[repr(packed)]`, `#[repr(transparent)]`, `#[repr(align(N))]`,
/// or any combination. Such attributes pin the struct's exact in-memory layout
/// (FFI, `bytemuck` byte-casts, alignment guarantees), so extracting shared
/// fields into a nested type would change the layout and break the contract;
/// the struct therefore cannot participate in a data clump.
///
/// Attributes are the struct's preceding `attribute_item` siblings; interleaved
/// comment siblings are skipped and unrelated attributes (`#[derive(...)]`) are
/// traversed past.
fn has_layout_repr_attr(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = struct_node.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if repr_attr_constrains_layout(s, source) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is a `#[repr(...)]` whose arguments contain a
/// layout-constraining token: `C`, `packed`, `transparent`, or `align` (the
/// latter two also in their argument-bearing forms `packed(N)` / `align(N)`).
/// Integer reprs (`#[repr(u8)]`) and non-`repr` attributes yield `false`.
fn repr_attr_constrains_layout(attribute_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut item_cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };
    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    if path.utf8_text(source) != Ok("repr") {
        return false;
    }
    let Some(token_tree) = attribute.child_by_field_name("arguments") else {
        return false;
    };
    let Ok(text) = token_tree.utf8_text(source) else {
        return false;
    };
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    inner.split(',').any(|tok| {
        let head = tok.trim().split('(').next().unwrap_or("").trim();
        matches!(head, "C" | "packed" | "transparent" | "align")
    })
}

/// If `ty` is a single `Arc<X>`, `Rc<X>`, or `Weak<X>` smart-pointer wrapper,
/// return its strength and the inner type text `X` (trimmed). Only the
/// outermost wrapper is stripped — the inner text keeps any nested generics
/// intact (`Arc<Mutex<Option<Ticker>>>` → `(Strong, "Mutex<Option<Ticker>>")`).
/// Qualified paths (`std::sync::Arc<X>`) and non-wrapper types return `None`.
fn smart_pointer_parts(ty: tree_sitter::Node, source: &[u8]) -> Option<(Strength, String)> {
    if ty.kind() != "generic_type" {
        return None;
    }
    let strength = match ty.child_by_field_name("type")?.utf8_text(source).ok()? {
        "Arc" | "Rc" => Strength::Strong,
        "Weak" => Strength::Weak,
        _ => return None,
    };
    // `type_arguments` text is exactly `<…>`; stripping its delimiters removes
    // only the outermost wrapper (tree-sitter guarantees the matching pair).
    let inner = ty
        .child_by_field_name("type_arguments")?
        .utf8_text(source)
        .ok()?
        .trim()
        .strip_prefix('<')?
        .strip_suffix('>')?
        .trim()
        .to_string();
    Some((strength, inner))
}

/// True when, for every field name in `subset`, both structs type it as a
/// smart pointer of opposite strength over the same inner type — i.e. one is
/// `Arc`/`Rc` and the other `Weak`, wrapping identical inner type text. All
/// subset fields must satisfy this for the pair to be a strong/weak ownership
/// counterpart rather than a data clump.
fn is_strong_weak_pair(
    subset: &[(String, bool)],
    a: &FxHashMap<String, (Strength, String)>,
    b: &FxHashMap<String, (Strength, String)>,
) -> bool {
    subset.iter().all(|(name, _)| match (a.get(name), b.get(name)) {
        (Some((strength_a, inner_a)), Some((strength_b, inner_b))) => {
            strength_a != strength_b && inner_a == inner_b
        }
        _ => false,
    })
}

/// Generate all sorted subsets of size `k` from `items`.
fn combinations(items: &[String], k: usize) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut combo = vec![0usize; k];
    fn recurse(
        items: &[String],
        k: usize,
        start: usize,
        combo: &mut Vec<usize>,
        depth: usize,
        result: &mut Vec<Vec<String>>,
    ) {
        if depth == k {
            result.push(combo[..k].iter().map(|&i| items[i].clone()).collect());
            return;
        }
        if start + (k - depth) > items.len() {
            return;
        }
        for i in start..items.len() {
            combo[depth] = i;
            recurse(items, k, i + 1, combo, depth + 1, result);
        }
    }
    recurse(items, k, 0, &mut combo, 0, &mut result);
    result
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_repeated_field_group() {
        let src = r#"
struct CreateUser {
    name: String,
    email: String,
    age: u32,
}
struct UpdateUser {
    name: String,
    email: String,
    age: u32,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_different_fields() {
        let src = r#"
struct User {
    name: String,
    email: String,
    age: u32,
}
struct Email {
    to: String,
    subject: String,
    body: String,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fewer_than_three_shared() {
        let src = r#"
struct Foo {
    a: String,
    b: String,
    c: u32,
}
struct Bar {
    a: String,
    b: String,
    d: u32,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_cfg_test_structs() {
        let src = r#"
struct Env {
    id: String,
    netns: Option<String>,
    new_pid_ns: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ArgVals<'a> {
        id: &'a str,
        netns: Option<&'a str>,
        new_pid_ns: bool,
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_owned_borrowed_pair_issue_1026() {
        let src = r#"
type SmallString = String;

pub struct Realm {
    scheme: SmallString,
    host: Option<SmallString>,
    port: Option<u16>,
}

pub struct RealmRef<'a> {
    scheme: &'a str,
    host: Option<&'a str>,
    port: Option<u16>,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_lifetime_struct_without_reference_fields() {
        let src = r#"
use std::borrow::Cow;

struct Owned {
    x: String,
    y: String,
    z: String,
}

struct Lazy<'a> {
    x: Cow<'a, str>,
    y: Cow<'a, str>,
    z: String,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_production_clumps() {
        let src = r#"
struct Env {
    id: String,
    netns: Option<String>,
    new_pid_ns: bool,
}

struct ArgVals {
    id: String,
    netns: Option<String>,
    new_pid_ns: bool,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_generic_param_combinators_issue_6202() {
        let src = r#"
use std::marker::PhantomData;

pub struct FoldMany0<F, G, Init, R> {
    parser: F,
    g: G,
    init: Init,
    r: PhantomData<R>,
}

pub struct FoldMany1<F, G, Init, R> {
    parser: F,
    g: G,
    init: Init,
    r: PhantomData<R>,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_concrete_typed_clump_issue_6202() {
        let src = r#"
struct CreateAccount {
    name: String,
    id: u64,
    email: String,
}

struct UpdateAccount {
    name: String,
    id: u64,
    email: String,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn concrete_field_in_generic_clump_still_flags() {
        let src = r#"
struct Left<T, U> {
    a: T,
    b: U,
    name: String,
}

struct Right<T, U> {
    a: T,
    b: U,
    name: String,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_arc_weak_ownership_pair_issue_6365() {
        let src = r#"
pub struct ProgressBar {
    state: Arc<Mutex<BarState>>,
    pos: Arc<AtomicPosition>,
    ticker: Arc<Mutex<Option<Ticker>>>,
}

pub struct WeakProgressBar {
    state: Weak<Mutex<BarState>>,
    pos: Weak<AtomicPosition>,
    ticker: Weak<Mutex<Option<Ticker>>>,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_identical_primitive_clump() {
        let src = r#"
struct Point {
    x: i32,
    y: i32,
    z: i32,
}

struct Vector {
    x: i32,
    y: i32,
    z: i32,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_arc_weak_with_different_inner_types() {
        let src = r#"
struct Strong {
    a: Arc<Foo>,
    b: Arc<Bar>,
    c: Arc<Baz>,
}

struct Weakish {
    a: Weak<Foo>,
    b: Weak<Other>,
    c: Weak<Baz>,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_repr_c_layout_structs_issue_6950() {
        let src = r#"
#[derive(Debug, Clone, Copy, NoUninit, CheckedBitPattern)]
#[repr(C)]
pub struct SetVectors {
    pub docid: DocumentId,
    pub embedder_id: u8,
    _padding: [u8; 3],
}

#[derive(Debug, Clone, Copy, NoUninit, CheckedBitPattern)]
#[repr(C)]
pub struct SetVector {
    pub docid: DocumentId,
    pub embedder_id: u8,
    pub extractor_id: u8,
    _padding: [u8; 2],
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_repr_packed_layout_structs() {
        let src = r#"
#[repr(packed)]
struct PackedA {
    a: u32,
    b: u16,
    c: u8,
}

#[repr(packed)]
struct PackedB {
    a: u32,
    b: u16,
    c: u8,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_repr_align_layout_structs() {
        let src = r#"
#[repr(align(8))]
struct AlignedA {
    a: u32,
    b: u32,
    c: u32,
}

#[repr(align(8))]
struct AlignedB {
    a: u32,
    b: u32,
    c: u32,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_repr_c_packed_combination_structs() {
        let src = r#"
#[repr(C, packed)]
struct ComboA {
    a: u32,
    b: u16,
    c: u8,
}

#[repr(C, packed)]
struct ComboB {
    a: u32,
    b: u16,
    c: u8,
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// One struct carries `#[repr(C)]`, the other is plain. The repr struct is
    /// exempt and never collected, so only one struct remains for the shared
    /// subset — a clump needs two, so nothing is flagged.
    #[test]
    fn one_repr_one_plain_does_not_flag_pair() {
        let src = r#"
#[repr(C)]
struct ReprStruct {
    a: u32,
    b: u32,
    c: u32,
}

struct PlainStruct {
    a: u32,
    b: u32,
    c: u32,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_plain_structs_with_only_derive() {
        let src = r#"
#[derive(Clone)]
struct Alpha {
    a: u32,
    b: u32,
    c: u32,
}

#[derive(Clone)]
struct Beta {
    a: u32,
    b: u32,
    c: u32,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    /// `#[repr(u8)]` is an integer discriminant repr, not a layout repr, so it
    /// is NOT exempt — these structs still form a clump. Locks the token-tree
    /// discriminator that distinguishes layout reprs from integer reprs.
    #[test]
    fn still_flags_repr_int_structs() {
        let src = r#"
#[repr(u8)]
struct IntA {
    a: u32,
    b: u32,
    c: u32,
}

#[repr(u8)]
struct IntB {
    a: u32,
    b: u32,
    c: u32,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    /// The two structs share the names `[major, minor, patch]`, but `minor` and
    /// `patch` are mandatory `u32` in one and `Option<u32>` in the other. Only
    /// `major` agrees on optionality, dropping the shared subset below the
    /// 3-field threshold, so no clump can be extracted.
    #[test]
    fn no_fp_on_optionality_mismatch_issue_7296() {
        let src = r#"
pub struct PackageVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub struct VersionBound {
    pub major: u32,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Optionality only excludes the disagreeing field: with four shared names
    /// where one differs in optionality, the remaining three agree and still
    /// form an extractable clump.
    #[test]
    fn still_flags_when_enough_fields_agree_on_optionality() {
        let src = r#"
struct Left {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

struct Right {
    a: u32,
    b: u32,
    c: u32,
    d: Option<u32>,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    /// Two definitions of one struct name under mutually-exclusive
    /// `#[cfg(feature = "v1")]` / `#[cfg(feature = "v2")]` gates are a single
    /// logical type re-declared per build variant — never present together.
    /// They share `[currency, payment_id, status]` only because the cfg-twin is
    /// double-counted; collapsing the twin drops the count below threshold.
    #[test]
    fn no_fp_on_cfg_gated_same_name_twins_issue_7870() {
        let src = r#"
#[cfg(feature = "v1")]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RefundResponse {
    pub refund_id: String,
    pub payment_id: String,
    pub currency: common_enums::Currency,
    pub status: RefundStatus,
}

#[cfg(feature = "v2")]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RefundResponse {
    pub id: common_utils::id_type::GlobalRefundId,
    pub payment_id: common_utils::id_type::GlobalPaymentId,
    pub currency: common_enums::Currency,
    pub status: RefundStatus,
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// The dedup collapses same-name cfg twins, not every cfg-gated struct: a
    /// `#[cfg]`-gated struct still forms a clump with a differently-named struct
    /// that repeats its field group, so genuine clumps are not suppressed.
    #[test]
    fn still_flags_cfg_gated_struct_clumping_with_distinct_struct() {
        let src = r#"
#[cfg(feature = "v1")]
struct RefundResponse {
    payment_id: String,
    currency: String,
    status: String,
}

struct PaymentResponse {
    payment_id: String,
    currency: String,
    status: String,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    /// `#[cfg_attr(...)]` conditionally applies an attribute but always compiles
    /// the item, so it is not a build-variant gate: two always-compiled same-name
    /// structs (here in separate modules) that share a field group are a genuine
    /// clump and must still be flagged.
    #[test]
    fn still_flags_cfg_attr_same_name_structs() {
        let src = r#"
mod a {
    #[cfg_attr(feature = "serde", derive(Serialize))]
    pub struct Config {
        pub host: String,
        pub port: String,
        pub tls: String,
    }
}

mod b {
    #[cfg_attr(feature = "serde", derive(Serialize))]
    pub struct Config {
        pub host: String,
        pub port: String,
        pub tls: String,
    }
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    /// Three same-name definitions under mutually-exclusive gates collapse to a
    /// single representative, so the shared field group never reaches the
    /// two-struct threshold.
    #[test]
    fn no_fp_on_three_way_cfg_gated_twins() {
        let src = r#"
#[cfg(feature = "v1")]
struct Handle {
    id: String,
    kind: String,
    owner: String,
}

#[cfg(feature = "v2")]
struct Handle {
    id: String,
    kind: String,
    owner: String,
}

#[cfg(feature = "v3")]
struct Handle {
    id: String,
    kind: String,
    owner: String,
}
"#;
        assert!(run_on(src).is_empty());
    }
}
