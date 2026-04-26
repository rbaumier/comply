//! dockerfile-no-curl-and-wget tree-sitter backend.
//!
//! Walks every `run_instruction` from the file root once and checks whether
//! both `curl` and `wget` appear anywhere across the file. If so, emits a
//! single diagnostic anchored at the run_instruction that introduced the
//! second of the two tools.

use crate::diagnostic::{Diagnostic, Severity};

fn contains_tool(text: &str, tool: &str) -> bool {
    // Match `tool` only when bordered by start/whitespace/&|;`(' on the left
    // and end/whitespace/&|;` on the right. Avoids matching `curlsh`, etc.
    let bytes = text.as_bytes();
    let tool_bytes = tool.as_bytes();
    let mut i = 0;
    while i + tool_bytes.len() <= bytes.len() {
        if &bytes[i..i + tool_bytes.len()] == tool_bytes {
            let left_ok = i == 0
                || matches!(
                    bytes[i - 1],
                    b' ' | b'\t' | b'\n' | b'&' | b'|' | b';' | b'`' | b'(' | b'\''
                );
            let right_idx = i + tool_bytes.len();
            let right_ok = right_idx == bytes.len()
                || matches!(
                    bytes[right_idx],
                    b' ' | b'\t' | b'\n' | b'&' | b'|' | b';' | b'`' | b')' | b'\''
                );
            if left_ok && right_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let mut has_curl = false;
    let mut has_wget = false;
    let mut second_node: Option<tree_sitter::Node> = None;

    let mut cursor = node.walk();
    for run in node.children(&mut cursor) {
        if run.kind() != "run_instruction" { continue; }
        let text = match std::str::from_utf8(&source[run.byte_range()]) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let this_curl = contains_tool(text, "curl");
        let this_wget = contains_tool(text, "wget");
        let prev_curl = has_curl;
        let prev_wget = has_wget;
        has_curl = has_curl || this_curl;
        has_wget = has_wget || this_wget;
        if second_node.is_none() && has_curl && has_wget && (prev_curl != has_curl || prev_wget != has_wget) {
            second_node = Some(run);
        }
    }

    if let Some(second) = second_node {
        let pos = second.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Dockerfile uses both curl and wget; pick one to reduce image size.".into(),
            severity: Severity::Warning,
            span: Some((second.byte_range().start, second.byte_range().len())),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_curl_then_wget() {
        let src = "FROM alpine\nRUN apk add curl\nRUN apk add wget\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_both_in_same_run() {
        let src = "FROM alpine\nRUN apk add curl wget\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_only_curl() {
        let src = "FROM alpine\nRUN apk add curl\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_only_wget() {
        let src = "FROM alpine\nRUN apk add wget\n";
        assert!(run(src).is_empty());
    }
}
