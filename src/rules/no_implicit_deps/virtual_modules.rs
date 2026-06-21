//! Project-registered virtual module ID extraction.
//!
//! Vite/Rollup/Nuxt plugins can register a *virtual module* whose ID looks like
//! an ordinary npm package name (`nuxt-vitest-environment-options`) yet is never
//! published: a plugin's `resolveId`/`load` hooks resolve the bare specifier to
//! generated code at build time. Such an ID legitimately appears in an `import`
//! without being a `package.json` dependency, so `no-implicit-deps` reads the
//! project's own source to learn which bare specifiers a local plugin registers.
//!
//! Detection is structural and conservative: a string literal counts as a
//! registered virtual ID only when it appears in a file that also defines a
//! plugin `resolveId` hook. `resolveId` is the hook that *creates* an importable
//! virtual ID (it returns a non-relative module ID for an otherwise-unresolvable
//! specifier); `load` alone only serves content for an already-resolved ID, so
//! requiring `resolveId` keeps the gate tight and avoids exempting a real missing
//! dependency that merely sits in a file with an unrelated `load` function. The
//! co-occurrence of the literal with the `resolveId` hook in the same project
//! source file is the evidence — a genuinely missing dependency has no such
//! registration and still fires.

use rustc_hash::FxHashSet;

/// True if `source` defines a Vite/Rollup plugin `resolveId` hook — the hook
/// that turns a bare specifier into a build-time virtual module by returning a
/// resolved module ID. Its presence marks a file as a plugin definition whose
/// string literals may be the virtual module IDs it registers.
fn defines_resolver_hook(source: &str) -> bool {
    contains_ident(source, "resolveId")
}

/// Collect every quoted string literal in a plugin-defining `source` file into
/// `out`. Only called for files that pass [`defines_resolver_hook`], so the
/// literals gathered are candidate virtual module IDs registered by that plugin.
///
/// Single- and double-quoted literals are scanned (template literals are skipped
/// — a virtual ID is a fixed string, never interpolated). Only literals shaped
/// like a bare specifier (a plausible package name) are kept, so paths, URLs,
/// and free-form messages do not pollute the set.
pub(crate) fn collect_virtual_ids(source: &str, out: &mut FxHashSet<String>) {
    if !defines_resolver_hook(source) {
        return;
    }
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i];
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != quote {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                if let Some(lit) = source.get(start..j)
                    && is_virtual_id_candidate(lit)
                {
                    out.insert(lit.to_string());
                }
                i = j + 1;
            }
            _ => i += 1,
        }
    }
}

/// True if `lit` is shaped like a bare package specifier, so it could be a
/// virtual module ID an `import` would reference. Must start with an
/// alphanumeric or `@` and contain only the characters that may appear in a
/// package name (`A-Z a-z 0-9 - _ / @ .`). This excludes relative/absolute
/// paths (leading `.`/`/`), URLs (a `://` substring), and any free-form string
/// carrying whitespace or punctuation outside that set. `.` is permitted because
/// virtual IDs may legitimately contain it (`uno.css`-style names).
fn is_virtual_id_candidate(lit: &str) -> bool {
    if lit.is_empty() {
        return false;
    }
    if lit.starts_with('.') || lit.starts_with('/') {
        return false;
    }
    if lit.contains("://") {
        return false;
    }
    let first = lit.as_bytes()[0];
    if !(first.is_ascii_alphanumeric() || first == b'@') {
        return false;
    }
    lit.bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'/' | b'@' | b'.'))
}

/// Whether `needle` occurs in `source` as a standalone identifier (not a
/// substring of a longer identifier), so `myResolveId` does not match
/// `resolveId`.
fn contains_ident(source: &str, needle: &str) -> bool {
    let bytes = source.as_bytes();
    let mut from = 0;
    while let Some(rel) = source[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident_char(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident_char(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(source: &str) -> FxHashSet<String> {
        let mut out = FxHashSet::default();
        collect_virtual_ids(source, &mut out);
        out
    }

    // Regression #5163: a Vite plugin that registers a virtual module whose ID
    // looks like a package name (`nuxt-vitest-environment-options`) via a
    // `resolveId` hook. The literal co-occurs with the hook, so it is recognized
    // as a project-registered virtual ID.
    #[test]
    fn extracts_id_from_resolver_hook() {
        let src = r#"
            const STUB_ID = 'nuxt-vitest-environment-options'
            export function plugin() {
              return {
                resolveId(id) { if (id.endsWith(STUB_ID)) return STUB_ID },
                load(id) { if (id.endsWith(STUB_ID)) return 'export default {}' },
              }
            }
        "#;
        assert!(ids(src).contains("nuxt-vitest-environment-options"));
    }

    // A file with no `resolveId` hook is not a virtual-module-registering plugin,
    // so none of its string literals are treated as virtual IDs — a genuinely
    // missing dep imported in ordinary code keeps firing.
    #[test]
    fn no_resolver_hook_yields_nothing() {
        let src = r#"import x from 'some-real-package'; const id = 'another-package';"#;
        assert!(ids(src).is_empty());
    }

    // `load` alone does not gate: it only serves content for an already-resolved
    // ID and is too common a name (a `load()` data helper) to mark a file as a
    // virtual-module plugin. A file with a `load` function and a package-shaped
    // literal must NOT register that literal — otherwise a real missing dep
    // matching it would be silently exempted.
    #[test]
    fn load_alone_does_not_register_ids() {
        let src = r#"async function load() { return 'genuinely-missing-package' }"#;
        assert!(ids(src).is_empty());
    }

    // `resolveId` must be a standalone identifier, not a substring, so a
    // `myResolveId` helper does not turn the file into a plugin definition.
    #[test]
    fn substring_hook_names_do_not_match() {
        let src = r#"function myResolveId(id) { return 'looks-like-a-package' }"#;
        assert!(ids(src).is_empty());
    }

    // Paths, URLs, and free-form strings inside a plugin file are not virtual
    // module IDs and must be excluded from the candidate set.
    #[test]
    fn excludes_non_specifier_literals() {
        let src = r#"
            export function plugin() {
              return {
                resolveId(id) {
                  console.log('resolving module from ./src/index.ts')
                  return 'https://example.com/x'
                },
              }
            }
        "#;
        let got = ids(src);
        assert!(
            got.iter().all(|s| s == "id"),
            "only specifier-shaped literals kept, got {got:?}"
        );
    }
}
