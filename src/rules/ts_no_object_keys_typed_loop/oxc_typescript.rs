use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn parse_simple_ident(s: &str) -> Option<(&str, usize)> {
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 { None } else { Some((&s[..end], end)) }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Object.keys("])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let mut search_from = 0;
            while let Some(rel) = line[search_from..].find("Object.keys(") {
                let start = search_from + rel;
                let after = &line[start + "Object.keys(".len()..];
                let Some((ident, ident_end)) = parse_simple_ident(after) else {
                    search_from = start + 1;
                    continue;
                };
                let rest = &after[ident_end..];
                let trimmed = rest.trim_start();
                let array_method = if trimmed.starts_with(").forEach(") {
                    Some(".forEach")
                } else if trimmed.starts_with(").map(") {
                    Some(".map")
                } else {
                    None
                };
                let Some(method) = array_method else {
                    search_from = start + 1;
                    continue;
                };
                let needle = format!("{ident}[");
                if line.contains(&needle) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: start + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`Object.keys({ident}){method}(...)` types `k` as `string`, so \
                             `{ident}[k]` widens to `any`. Use `Object.entries({ident})` or cast: \
                             `(Object.keys({ident}) as Array<keyof typeof {ident}>){method}(...)`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                search_from = start + "Object.keys(".len();
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_object_keys_foreach_index() {
        let src = "Object.keys(obj).forEach(k => console.log(obj[k]));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_object_keys_map_index() {
        let src = "const r = Object.keys(state).map(k => state[k] + 1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_entries() {
        let src = "Object.entries(obj).forEach(([k, v]) => console.log(v));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_object_keys_without_index() {
        let src = "Object.keys(obj).forEach(k => log(k));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_of_entries() {
        let src = "for (const [k, v] of Object.entries(obj)) { log(k, v); }";
        assert!(run(src).is_empty());
    }
}
