use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn has_jsx(source: &str) -> bool {
    let mut from = 0usize;
    while let Some(rel) = source[from..].find('<') {
        let abs = from + rel;
        let next = source.as_bytes().get(abs + 1).copied();
        if let Some(c) = next
            && (c.is_ascii_uppercase() || c.is_ascii_lowercase())
        {
            return true;
        }
        from = abs + 1;
    }
    false
}

fn is_inside_jsx_event_handler(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut handler_starts: Vec<usize> = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'o' && bytes[i + 1] == b'n' {
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$');
            if prev_ok && bytes[i + 2].is_ascii_uppercase() {
                let mut j = i + 2;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'$')
                {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'=' && bytes[j + 1] == b'{' {
                    handler_starts.push(j + 1);
                    i = j + 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    let Some(&start) = handler_starts.last() else {
        return false;
    };
    let mut depth: i32 = 1;
    let mut k = start + 1;
    while k < bytes.len() {
        match bytes[k] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
            _ => {}
        }
        k += 1;
    }
    depth > 0
}

fn looks_like_in_component_render(source: &str, parse_offset: usize) -> bool {
    let preceding = &source[..parse_offset];
    let mut look_start = preceding.len().saturating_sub(2048);
    while look_start > 0 && !preceding.is_char_boundary(look_start) {
        look_start -= 1;
    }
    let snippet = &preceding[look_start..];

    let mut near = preceding.len().saturating_sub(500);
    while near > 0 && !preceding.is_char_boundary(near) {
        near -= 1;
    }
    let near_snippet = &preceding[near..];
    if near_snippet
        .rfind("useMemo(")
        .map(|p| p > near_snippet.rfind("})").unwrap_or(0))
        .unwrap_or(false)
    {
        return false;
    }
    if near_snippet
        .rfind("useCallback(")
        .map(|p| p > near_snippet.rfind("})").unwrap_or(0))
        .unwrap_or(false)
    {
        return false;
    }
    if is_inside_jsx_event_handler(near_snippet) {
        return false;
    }
    for keyword in ["function ", "const "] {
        let mut from = 0usize;
        while let Some(rel) = snippet[from..].find(keyword) {
            let pos = from + rel;
            let after = &snippet[pos + keyword.len()..];
            let bs = after.as_bytes();
            let mut k = 0usize;
            while k < bs.len() && (bs[k].is_ascii_alphanumeric() || bs[k] == b'_' || bs[k] == b'$')
            {
                k += 1;
            }
            if k > 0 && bs[0].is_ascii_uppercase() {
                return true;
            }
            from = pos + keyword.len();
        }
    }
    false
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find(".parse(") {
        let abs = from + rel;
        let mut prev_window_start = abs.saturating_sub(20);
        while prev_window_start > 0 && !source.is_char_boundary(prev_window_start) {
            prev_window_start -= 1;
        }
        let prev = &source[prev_window_start..abs];
        if prev.ends_with("JSON") {
            from = abs + 1;
            continue;
        }
        if looks_like_in_component_render(source, abs) {
            out.push(abs);
        }
        from = abs + 1;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".parse("])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains(".parse(") {
            return Vec::new();
        }
        if !has_jsx(ctx.source) {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`.parse(...)` in a render path re-validates every render and throws on bad data — \
                              move validation to the data fetch boundary or `useMemo`."
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
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_parse_in_component() {
        let src =
            "function Comp(props) { const data = Schema.parse(props.input); return <div>{data}</div>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_parse_in_arrow_component() {
        let src = "const Comp = (props) => { const data = Schema.parse(props.input); return <div /> }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_json_parse() {
        let src =
            "function Comp() { const data = JSON.parse(raw); return <div>{data}</div>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_parse_outside_component() {
        let src = "function loadConfig() { return Schema.parse(env); }\nfunction Comp() { return <div /> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_parse_inside_jsx_event_handler() {
        let src = "function Comp() { return <Radio onValueChange={(v) => SessionLevelSchema.parse(v)} /> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_parse_inside_on_click() {
        let src = "function Comp() { return <button onClick={() => Schema.parse(input)}>x</button> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_parse_outside_event_handler_in_component() {
        let src =
            "function Comp(props) { const data = Schema.parse(props.input); return <div />; }";
        assert_eq!(run(src).len(), 1);
    }
}
