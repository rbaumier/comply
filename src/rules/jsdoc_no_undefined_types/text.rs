use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

/// Extract the type from `{Type}` in a JSDoc tag.
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
    // Exact match.
    if KNOWN_TYPES.contains(&name) {
        return true;
    }
    // `*` is the any/wildcard type in JSDoc.
    if name == "*" {
        return true;
    }
    // Allow `...Type` (rest/spread in JSDoc).
    if let Some(inner) = name.strip_prefix("...") {
        return is_known_type(inner);
    }
    // Allow `?Type` (nullable in JSDoc).
    if let Some(inner) = name.strip_prefix('?') {
        return is_known_type(inner);
    }
    // Allow `!Type` (non-nullable in JSDoc).
    if let Some(inner) = name.strip_prefix('!') {
        return is_known_type(inner);
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
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
                            line: idx + 1,
                            column: 1,
                            rule_id: "jsdoc-no-undefined-types".into(),
                            message: format!(
                                "JSDoc type `{leaf}` is not a known built-in. Check for typos or add an import/typedef."
                            ),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_unknown_type() {
        let source = r#"
/**
 * @param {Strng} name - the name
 */
function greet(name) {}
"#;
        let d = run(source);
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
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_in_union() {
        let source = r#"
/**
 * @param {string|Nubmer} x
 */
function foo(x) {}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Nubmer"));
    }
}
