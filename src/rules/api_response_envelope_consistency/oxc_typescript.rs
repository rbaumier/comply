use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ROUTE_HINTS: &[&str] = &["route", "api", "handler", "controller", "endpoint"];

fn looks_like_api_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    ROUTE_HINTS.iter().any(|h| s.contains(h))
}

fn matching_brace(source: &[u8], open: usize) -> Option<usize> {
    if source.get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open + 1;
    while i < source.len() {
        match source[i] {
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

#[derive(Clone, Copy)]
enum Shape {
    Envelope,
    Raw,
}

fn has_top_level_data_key(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut depth_brace = 0i32;
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth_brace += 1,
            b'}' => depth_brace -= 1,
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'[' => depth_brack += 1,
            b']' => depth_brack -= 1,
            _ => {}
        }
        if depth_brace == 0
            && depth_paren == 0
            && depth_brack == 0
            && body.is_char_boundary(i)
            && (i == 0
                || !bytes[i - 1].is_ascii_alphanumeric()
                    && bytes[i - 1] != b'_'
                    && bytes[i - 1] != b'$')
            && body[i..].starts_with("data")
        {
            let after_idx = i + "data".len();
            if body.is_char_boundary(after_idx) {
                let trimmed = body[after_idx..].trim_start();
                if trimmed.starts_with(':') {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn collect_responses(source: &str) -> Vec<(usize, Shape)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let triggers = ["return {", "c.json({", "res.json({", "Response.json({"];
    for trigger in triggers {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(trigger) {
            let abs = from + rel;
            let brace_offset = abs + trigger.len() - 1;
            let Some(close) = matching_brace(bytes, brace_offset) else {
                break;
            };
            let body = &source[brace_offset + 1..close];
            let shape = if has_top_level_data_key(body) {
                Shape::Envelope
            } else {
                Shape::Raw
            };
            out.push((abs, shape));
            from = close + 1;
        }
    }
    out.sort_by_key(|(o, _)| *o);
    out
}

fn find_offenses(source: &str) -> Vec<usize> {
    let responses = collect_responses(source);
    let has_envelope = responses.iter().any(|(_, s)| matches!(s, Shape::Envelope));
    let has_raw = responses.iter().any(|(_, s)| matches!(s, Shape::Raw));
    if !(has_envelope && has_raw) {
        return Vec::new();
    }
    let envelope_count = responses
        .iter()
        .filter(|(_, s)| matches!(s, Shape::Envelope))
        .count();
    let raw_count = responses.len() - envelope_count;
    let minority_is_envelope = envelope_count <= raw_count;
    responses
        .into_iter()
        .filter(|(_, s)| {
            if minority_is_envelope {
                matches!(s, Shape::Envelope)
            } else {
                matches!(s, Shape::Raw)
            }
        })
        .map(|(o, _)| o)
        .collect()
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["return {", "c.json({", "res.json({"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !looks_like_api_path(ctx.path) {
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
                    message: "Response shape disagrees with other returns in this file — pick \
                              one envelope and stick to it."
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_mixed_shapes() {
        let src = "function a() { return { data: 1 }; }\nfunction b() { return { id: 2 }; }";
        assert!(!run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn allows_consistent_envelope() {
        let src = "function a() { return { data: 1 }; }\nfunction b() { return { data: 2 }; }";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn allows_consistent_raw() {
        let src = "function a() { return { id: 1 }; }\nfunction b() { return { id: 2 }; }";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn no_crash_on_emoji() {
        let src = "// 💡 Tip\nfunction a() { return { data: 1 }; }\nfunction b() { return { id: 2 }; }";
        let _ = run_at(src, "src/routes/api.ts");
    }

    #[test]
    fn ignores_non_api_files() {
        let src = "function a() { return { data: 1 }; }\nfunction b() { return { id: 2 }; }";
        assert!(run_at(src, "src/lib/util.ts").is_empty());
    }
}
