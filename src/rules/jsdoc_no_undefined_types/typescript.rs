//! jsdoc-no-undefined-types backend — flag unknown types in JSDoc tags.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Known built-in / common types that should not be flagged.
const KNOWN_TYPES: &[&str] = &[
    // Primitives.
    "string", "number", "boolean", "bigint", "symbol", "undefined", "null", "void",
    "never", "any", "unknown", "object", "this",
    // Built-in objects.
    "Array", "Object", "Function", "Promise", "Date", "RegExp", "Error",
    "Map", "Set", "WeakMap", "WeakSet", "ArrayBuffer", "SharedArrayBuffer",
    "DataView", "Int8Array", "Uint8Array", "Uint8ClampedArray",
    "Int16Array", "Uint16Array", "Int32Array", "Uint32Array",
    "Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array",
    "Buffer", "URL", "URLSearchParams",
    "ReadableStream", "WritableStream", "TransformStream",
    "Request", "Response", "Headers", "FormData", "Blob", "File",
    "AbortController", "AbortSignal",
    "Event", "EventTarget", "CustomEvent",
    "HTMLElement", "Element", "Node", "Document", "Window",
    "NodeJS", "Console", "JSON", "Math",
    "Record", "Partial", "Required", "Readonly", "Pick", "Omit",
    "Exclude", "Extract", "NonNullable", "ReturnType", "Parameters",
    "ConstructorParameters", "InstanceType", "ThisParameterType",
    "OmitThisParameter", "ThisType", "Awaited",
    "Iterator", "IterableIterator", "AsyncIterableIterator",
    "Generator", "AsyncGenerator",
    "Iterable", "AsyncIterable",
    "PromiseLike", "PropertyKey",
    "TypeError", "RangeError", "SyntaxError", "ReferenceError",
    "EvalError", "URIError",
    "Proxy", "Reflect",
    "WeakRef", "FinalizationRegistry",
    "TextEncoder", "TextDecoder",
    "MessageChannel", "MessagePort",
    "StructuredSerializeOptions",
    "true", "false",
];

/// Extract the type from `{Type}` in a JSDoc tag line.
fn extract_type_from_braces(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s[start..].find('}')? + start;
    Some(s[start + 1..end].trim())
}

/// Split a compound type into its leaf types (handles `|`, `&`, generics).
fn leaf_types(type_str: &str) -> Vec<String> {
    let mut leaves = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;

    for ch in type_str.chars() {
        match ch {
            '<' | '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            '|' | '&' if depth == 0 => {
                let t = current.trim().to_string();
                if !t.is_empty() {
                    leaves.push(t);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let t = current.trim().to_string();
    if !t.is_empty() {
        leaves.push(t);
    }

    // Strip generics: `Array<string>` -> `Array`.
    leaves
        .into_iter()
        .map(|t| {
            if let Some(idx) = t.find('<') {
                t[..idx].trim().to_string()
            } else if t.ends_with("[]") {
                t[..t.len() - 2].trim().to_string()
            } else {
                t
            }
        })
        .filter(|t| !t.is_empty())
        .collect()
}

fn is_known_type(name: &str) -> bool {
    if KNOWN_TYPES.contains(&name) {
        return true;
    }
    if name == "*" {
        return true;
    }
    if let Some(inner) = name.strip_prefix("...") {
        return is_known_type(inner);
    }
    if let Some(inner) = name.strip_prefix('?') {
        return is_known_type(inner);
    }
    if let Some(inner) = name.strip_prefix('!') {
        return is_known_type(inner);
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "comment" {
                return;
            }
            let Ok(text) = node.utf8_text(source_bytes) else { return };
            if !text.starts_with("/**") {
                return;
            }

            let comment_start_line = node.start_position().row;

            for (rel_idx, line) in text.lines().enumerate() {
                let content = line.trim().trim_start_matches('*').trim();

                let is_typed_tag = content.starts_with("@param")
                    || content.starts_with("@returns")
                    || content.starts_with("@return ")
                    || content.starts_with("@type")
                    || content.starts_with("@typedef")
                    || content.starts_with("@property")
                    || content.starts_with("@prop ");

                if !is_typed_tag {
                    continue;
                }

                if let Some(type_str) = extract_type_from_braces(content) {
                    for leaf in leaf_types(type_str) {
                        if !is_known_type(&leaf) {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: comment_start_line + rel_idx + 1,
                                column: 1,
                                rule_id: "jsdoc-no-undefined-types".into(),
                                message: format!(
                                    "JSDoc type `{leaf}` is not a known built-in. \
                                     Check for typos or add an import/typedef."
                                ),
                                severity: Severity::Warning,
                            });
                        }
                    }
                }
            }
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unknown_type() {
        let source = r#"
/**
 * @param {Strng} name - the name
 */
function greet(name) {}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Strng"));
    }

    #[test]
    fn allows_known_types() {
        let source = r#"
/**
 * @param {string} name - the name
 * @param {number|boolean} value - the value
 * @returns {Promise<Array>} result
 */
function process(name, value) {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_in_union() {
        let source = r#"
/**
 * @param {string|Nubmer} x
 */
function foo(x) {}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Nubmer"));
    }
}
