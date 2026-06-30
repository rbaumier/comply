//! Shared parameter/local shadow-scope tracking for the Vue ref-misuse text
//! rules (`vue-no-ref-as-operand`, `vue-ref-value-in-script`).
//!
//! A `ref()`/`computed()` binding declared at module scope is shadowed inside a
//! function or arrow body by a same-named parameter, or by a `const`/`let`/`var`
//! local declared earlier in that body. Inside such a scope the bare name is the
//! plain local value, not the Ref, so a bare-name usage there is correct and must
//! not be flagged. `collect_shadow_scopes` finds those scopes by byte range so
//! each rule can suppress a match whose offset falls inside one.

use rustc_hash::FxHashSet;

/// A block-bodied `function`/arrow scope: the byte range of its `{ … }` body,
/// the bare parameter names that shadow any outer binding inside it, and the
/// local `const`/`let`/`var` declarations inside the body that shadow an outer
/// name. Each local is `(name, decl_offset)`, where `decl_offset` is the
/// absolute byte offset of the declared identifier; a local shadows the outer
/// ref only for usages textually after its declaration.
pub(crate) struct ShadowScope {
    pub(crate) body: std::ops::Range<usize>,
    pub(crate) params: FxHashSet<String>,
    pub(crate) locals: Vec<(String, usize)>,
}

/// Whether `byte` is part of a JS/TS identifier (so we can require word
/// boundaries when matching the `function` keyword).
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Find the matching closing delimiter for the opener at `open` (a `(` or `{`),
/// returning the index of the closer. Returns `None` if unbalanced.
fn match_delimiter(bytes: &[u8], open: usize) -> Option<usize> {
    let (opener, closer) = (bytes[open], match bytes[open] {
        b'(' => b')',
        b'{' => b'}',
        _ => return None,
    });
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if b == opener {
            depth += 1;
        } else if b == closer {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Extract every parameter binding identifier from a parameter list (the text
/// between the parens). Splits on depth-0 commas, then for each parameter
/// collects all names it binds — including those introduced by array
/// destructuring (`[a, b, ...rest]`, nested `[a, [b, c]]`) and object
/// destructuring (`{ a, b }`, renamed `{ a: b }` → the bound name `b`, nested
/// `{ a: { b } }`, `...rest`). Default-value expressions are ignored.
fn parse_param_names(param_list: &str) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for seg in split_top_level_commas(param_list) {
        collect_pattern_idents(seg, &mut names);
    }
    names
}

/// Split `s` on commas that sit at bracket/brace/paren/angle depth 0, returning
/// the segments. Angle brackets are tracked so commas inside a generic type
/// annotation (`Map<string, number>`) don't split a parameter.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut segments = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b',' if depth == 0 => {
                segments.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&s[start..]);
    segments
}

/// Add every binding identifier introduced by one parameter (or nested
/// destructuring) pattern to `names`. Handles a plain identifier (with optional
/// `: Type` annotation and `= default`), array destructuring `[a, b, ...rest]`
/// (incl. nested `[a, [b, c]]`), and object destructuring `{ a, b }` (shorthand
/// → `a`/`b`), renamed `{ a: b }` (the BOUND name `b`, not the key), nested
/// `{ a: { b } }`, and `...rest`. A default-value expression (after `=`) is not
/// descended into, so a default like `= [1, 2]` or `= inner` cannot leak a
/// phantom identifier into the shadow set.
fn collect_pattern_idents(pattern: &str, names: &mut FxHashSet<String>) {
    let trimmed = pattern.trim().trim_start_matches("...").trim_start();
    match trimmed.as_bytes().first() {
        Some(b'[') => {
            // Array pattern: each element is itself a binding target.
            if let Some(inner) = bracket_interior(trimmed) {
                for elem in split_top_level_commas(inner) {
                    collect_pattern_idents(elem, names);
                }
            }
        }
        Some(b'{') => {
            // Object pattern: each property is `key`, `key: target`, or
            // `key = default`. The bound name is the target after a rename
            // colon when present, otherwise the key.
            if let Some(inner) = bracket_interior(trimmed) {
                for prop in split_top_level_commas(inner) {
                    let prop = prop.trim().trim_start_matches("...").trim_start();
                    match object_prop_target(prop) {
                        Some(target) => collect_pattern_idents(target, names),
                        None => push_leading_ident(prop, names),
                    }
                }
            }
        }
        _ => push_leading_ident(trimmed, names),
    }
}

/// Insert the leading identifier of `s` (already trimmed and spread-stripped)
/// into `names`. Stops at the first non-identifier byte, so a `: Type`
/// annotation or `= default` suffix is excluded.
fn push_leading_ident(s: &str, names: &mut FxHashSet<String>) {
    let ident: String = s
        .chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        .collect();
    if !ident.is_empty() {
        names.insert(ident);
    }
}

/// For a string beginning with `[` or `{`, return the slice strictly inside its
/// matching close delimiter, ignoring any trailing `: Type` / `= default`.
/// Tracks all three bracket pairs so a nested pattern (`{ a: [b] }`) closes
/// correctly. Returns `None` if unbalanced.
fn bracket_interior(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// For one object-destructuring property, return the rename target (the text
/// after a depth-0 `:`) when the property renames its key (`key: target`).
/// Returns `None` for a shorthand property (`key` / `key = default`); a `=`
/// reached before any `:` means there is no rename colon.
fn object_prop_target(prop: &str) -> Option<&str> {
    let bytes = prop.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b':' if depth == 0 => return Some(&prop[i + 1..]),
            b'=' if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

/// Scan a scope body for local `const`/`let`/`var` declarations that shadow an
/// outer name. Returns `(name, decl_offset)` where `decl_offset` is the absolute
/// byte offset (`base + local_offset`) of the declared identifier. Destructuring
/// declarations (`const { x }` / `const [x]`) yield an empty identifier (the byte
/// after the keyword is `{`/`[`) and are skipped, mirroring `parse_param_names`.
fn collect_local_decls(body: &str, base: usize) -> Vec<(String, usize)> {
    let bytes = body.as_bytes();
    let mut locals = Vec::new();
    for kw in ["const", "let", "var"] {
        for (kw_pos, _) in body.match_indices(kw) {
            // Require word boundaries so `letter`/`constant` don't match.
            let before_ok = kw_pos == 0 || !is_ident_byte(bytes[kw_pos - 1]);
            let after_kw = kw_pos + kw.len();
            let after_ok = after_kw >= bytes.len() || !is_ident_byte(bytes[after_kw]);
            if !before_ok || !after_ok {
                continue;
            }
            // Skip whitespace, then read the declared identifier.
            let mut i = after_kw;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let name_start = i;
            let ident: String = body[name_start..]
                .chars()
                .take_while(|&c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
                .collect();
            if !ident.is_empty() {
                locals.push((ident, base + name_start));
            }
        }
    }
    locals
}

/// Collect block-bodied `function …(params) { … }` and arrow `(params) => { … }`
/// shadow scopes in `source`, with byte offsets absolute to `source`. A usage
/// whose offset lies inside a scope's body is shadowing the outer ref — and must
/// not be flagged — when its name is one of that scope's params, or matches a
/// local `const`/`let`/`var` declaration of the same name that appears textually
/// before the usage.
pub(crate) fn collect_shadow_scopes(source: &str) -> Vec<ShadowScope> {
    let bytes = source.as_bytes();
    let mut scopes = Vec::new();

    // `function`-keyword forms: `function name(params) {` and
    // `function (params) {` (anonymous).
    for (kw, _) in source.match_indices("function") {
        let before_ok = kw == 0 || !is_ident_byte(bytes[kw - 1]);
        let after = kw + "function".len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if !before_ok || !after_ok {
            continue;
        }
        if let Some(scope) = scope_from_params_at(source, after) {
            scopes.push(scope);
        }
    }

    // Arrow forms: a `(params) => {` whose `=>` is immediately followed by a
    // block body. Anchor on `=>` then walk back to the param-list parens.
    for (arrow, _) in source.match_indices("=>") {
        let mut j = arrow + 2;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'{' {
            continue;
        }
        // Walk back over whitespace to the `)` that closes the param list.
        let mut k = arrow;
        while k > 0 && bytes[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        if k == 0 || bytes[k - 1] != b')' {
            continue;
        }
        let close_paren = k - 1;
        let Some(open_paren) = find_matching_open(bytes, close_paren) else {
            continue;
        };
        let params = parse_param_names(&source[open_paren + 1..close_paren]);
        let Some(body_close) = match_delimiter(bytes, j) else {
            continue;
        };
        scopes.push(ShadowScope {
            body: j..body_close,
            params,
            locals: Vec::new(),
        });
    }

    // Populate each scope's local `const`/`let`/`var` declarations from its body
    // text, recording absolute offsets so position checks line up with usage
    // offsets at the call site.
    for scope in &mut scopes {
        scope.locals = collect_local_decls(&source[scope.body.clone()], scope.body.start);
    }

    scopes
}

/// Build a shadow scope from the position just after a `function` keyword:
/// skips the optional name, reads the `(params)` list, then the `{ … }` body.
fn scope_from_params_at(source: &str, after_kw: usize) -> Option<ShadowScope> {
    let bytes = source.as_bytes();
    let mut i = after_kw;
    // Skip whitespace, an optional `*` (generator) and the optional name.
    while i < bytes.len() && bytes[i] != b'(' {
        if bytes[i] == b'{' {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let open_paren = i;
    let close_paren = match_delimiter(bytes, open_paren)?;
    let params = parse_param_names(&source[open_paren + 1..close_paren]);
    // Find the `{` that opens the body (skipping an optional `: ReturnType`).
    let mut b = close_paren + 1;
    while b < bytes.len() && bytes[b] != b'{' {
        b += 1;
    }
    if b >= bytes.len() {
        return None;
    }
    let body_close = match_delimiter(bytes, b)?;
    Some(ShadowScope {
        body: b..body_close,
        params,
        locals: Vec::new(),
    })
}

/// Find the `(` matching the `)` at `close` by scanning backwards.
fn find_matching_open(bytes: &[u8], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = close;
    loop {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}
