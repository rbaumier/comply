use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const PENDING_FLAGS: &[&str] = &["isPending", "isLoading"];

/// Find balanced close brace for a block starting at `open_brace_offset`.
fn find_closing_brace(source: &str, open_brace_offset: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open_brace_offset) != Some(&b'{') {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open_brace_offset + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// True if `body` contains an `if (ispending/isloading...) return` pattern
/// for identifiers in `idents`.
fn has_identifier_guard(body: &str, idents: &[&str]) -> bool {
    let mut from = 0usize;
    while let Some(rel) = body[from..].find("if") {
        let abs = from + rel;
        // Word boundary
        let pre = body.as_bytes().get(abs.saturating_sub(1)).copied();
        let post = body.as_bytes().get(abs + 2).copied();
        if pre.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !post.map_or(true, |c| c.is_ascii_whitespace() || c == b'(')
        {
            from = abs + 1;
            continue;
        }
        // Extract the condition: look for `(...)`.
        let after_if = abs + 2;
        let Some(cond_rel) = body[after_if..].find('(') else {
            from = abs + 1;
            continue;
        };
        let cond_start = after_if + cond_rel;
        let cond_text = &body[cond_start..];
        // Check if condition contains any of our identifiers.
        let has_ident = idents.iter().any(|name| {
            let mut from2 = 0usize;
            while let Some(rel2) = cond_text[from2..].find(name) {
                let pos2 = from2 + rel2;
                let pre2 = cond_text.as_bytes().get(pos2.saturating_sub(1)).copied();
                let post2 = cond_text.as_bytes().get(pos2 + name.len()).copied();
                if pre2.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$')
                    && post2.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$')
                {
                    return true;
                }
                from2 = pos2 + 1;
            }
            false
        });
        if !has_ident {
            from = abs + 1;
            continue;
        }
        // Check that the if-body contains a return.
        let after_cond = cond_start + cond_text.find(')').map_or(cond_text.len(), |p| p + 1);
        let rest = body[after_cond..].trim_start();
        let has_return = if rest.starts_with('{') {
            let end = find_closing_brace(rest, 0).unwrap_or(rest.len());
            rest[..end].contains("return")
        } else {
            rest.starts_with("return")
        };
        if has_return {
            return true;
        }
        from = abs + 1;
    }
    None::<()>; // unreachable but silence warning
    false
}

/// True if `body` contains `if (<obj>.isPending|isLoading) return ...`.
fn has_member_guard(body: &str, obj_name: &str) -> bool {
    for flag in PENDING_FLAGS {
        let pat = format!("{obj_name}.{flag}");
        if has_identifier_guard(body, &[pat.as_str()]) {
            return true;
        }
    }
    false
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("useQuery") {
        let abs = from + rel;
        // Word boundary check: must be `useQuery(` not `useSuspenseQuery`
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        let post = source.as_bytes().get(abs + "useQuery".len()).copied();
        if pre.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$') {
            from = abs + 1;
            continue;
        }
        if post != Some(b'(') && post != Some(b'<') {
            from = abs + 1;
            continue;
        }
        // Look backward for either:
        // `const { isPending, ... } = ` or `const <id> = `
        let preceding = &source[..abs];
        let mut look_start = preceding.len().saturating_sub(300);
        while look_start > 0 && !preceding.is_char_boundary(look_start) {
            look_start -= 1;
        }
        let snippet = &preceding[look_start..];
        // Find any = sign right before useQuery
        if let Some(eq_pos) = snippet.rfind('=') {
            let before_eq = snippet[..eq_pos].trim_end();
            // Destructured: `const { isPending`
            let pending_destructured: Vec<&str> = PENDING_FLAGS
                .iter()
                .filter(|flag| before_eq.contains(*flag))
                .map(|f| *f)
                .collect();

            // Find enclosing function body to scan for guard
            // Look forward for the enclosing `{...}` block
            let fn_body_start = source[abs..].find('{').map(|p| abs + p);
            if let Some(body_start) = fn_body_start {
                // Try to find a wider function scope
                // Walk back to find nearest function body that contains `abs`
                let fn_body = find_enclosing_function_body(source, abs);

                if let Some((body_text, _body_offset)) = fn_body {
                    if !pending_destructured.is_empty() {
                        if has_identifier_guard(body_text, &pending_destructured) {
                            out.push(abs);
                            from = abs + 1;
                            continue;
                        }
                    } else {
                        // Non-destructured: `const q = useQuery(...)` → check `q.isPending`
                        // Extract identifier name
                        let id_part = before_eq.trim_start_matches(|c: char| {
                            c == '\n' || c == '\r' || c == '\t' || c == ' '
                        });
                        // Get the last word
                        let mut id_name = "";
                        for kw in ["const ", "let ", "var "] {
                            if let Some(pos) = id_part.rfind(kw) {
                                let after = &id_part[pos + kw.len()..];
                                let end = after
                                    .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                                    .unwrap_or(after.len());
                                id_name = &after[..end];
                                break;
                            }
                        }
                        if !id_name.is_empty() && has_member_guard(body_text, id_name) {
                            out.push(abs);
                        }
                    }
                }
                let _ = body_start;
            }
        }
        from = abs + 1;
    }
    out
}

fn find_enclosing_function_body(source: &str, pos: usize) -> Option<(&str, usize)> {
    // Walk backwards looking for function/arrow function bodies
    let before = &source[..pos];
    // Find the last `{` that's part of a function body
    // Heuristic: find the outermost `{` that brackets pos
    let mut depth: i32 = 0;
    let bytes = source.as_bytes();
    for i in (0..pos).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    // Found an opening brace before pos
                    let close = find_closing_brace(source, i)?;
                    if close > pos {
                        return Some((&source[i..=close], i));
                    }
                } else {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    let _ = before;
    None
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useQuery"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("useQuery") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`useQuery` with an `if (isPending|isLoading) return …` guard \
                              should use `useSuspenseQuery` and a `<Suspense>` boundary — \
                              `data` will be guaranteed defined."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_destructured_is_pending_with_early_return() {
        let diags = run(
            "function C() {
                const { isPending, data } = useQuery({ queryKey: ['x'], queryFn: f });
                if (isPending) return null;
                return data;
            }",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn flags_destructured_is_loading() {
        let diags = run(
            "function C() {
                const { isLoading, data } = useQuery({ queryKey: ['x'], queryFn: f });
                if (isLoading) return null;
                return data;
            }",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn flags_non_destructured_member_access() {
        let diags = run(
            "function C() {
                const q = useQuery({ queryKey: ['x'], queryFn: f });
                if (q.isPending) return null;
                return q.data;
            }",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn allows_use_suspense_query() {
        let diags = run(
            "function C() {
                const { data } = useSuspenseQuery({ queryKey: ['x'], queryFn: f });
                return data;
            }",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_use_infinite_query() {
        let diags = run(
            "function C() {
                const { isPending, data } = useInfiniteQuery({ queryKey: ['x'], queryFn: f });
                if (isPending) return null;
                return data;
            }",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }
}
