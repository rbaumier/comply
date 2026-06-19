//! csv-formula-injection OXC backend — flag a dynamic CSV cell that is joined
//! into a row without a formula-escape (OWASP CSV injection).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Row separators that mark a `.join(...)` as building a CSV row. A
/// comma-or-semicolon join is the de-facto CSV serialization shape; any other
/// separator (`""`, `" "`, `"/"`, `"\n"`) is not a CSV row.
const CSV_SEPARATORS: &[&str] = &[",", ";"];

/// Callee names that already neutralize a leading formula character. A cell
/// wrapped in one of these is escaped, so it is never flagged. Mirrors the
/// hardcoded-allowlist convention used by sibling security rules.
const ESCAPE_HELPERS: &[&str] = &[
    "escapecsv",
    "csvescape",
    "sanitizecsv",
    "guardcsvrow",
    "escapeformula",
    "escapecsvcell",
    "csvsanitize",
    "csvcell",
    "formulaescape",
    "neutralizeformula",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".join"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // The sink is `<array>.join("," | ";")`. The receiver array holds the
        // cells; the separator string distinguishes a CSV row from an arbitrary
        // join.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "join" {
            return;
        }
        if !separator_is_csv(call) {
            return;
        }
        let Expression::ArrayExpression(array) = &member.object else {
            return;
        };

        // FP gate (the crux): a bare `.join(",")` is ubiquitous. Fire only when a
        // CSV context is established — a `text/csv` response emitted in the
        // enclosing function, or a csv-named binding/function around this join.
        // Without that signal, stay silent.
        if !in_csv_context(node, semantic) {
            return;
        }

        // Flag the first cell that is a dynamic, unescaped string expression.
        let Some(cell) = array.elements.iter().find(|el| is_unescaped_dynamic_cell(el)) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, cell.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Dynamic CSV cell is joined into a row without a formula-escape — wrap it (e.g. `escapeCsv(...)`) to neutralize a leading `=`/`+`/`-`/`@`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the sole argument of `.join(...)` is a `","` or `";"` string
/// literal — the CSV row separators.
fn separator_is_csv(call: &oxc_ast::ast::CallExpression) -> bool {
    let Some(arg) = call.arguments.first() else {
        return false;
    };
    let Some(Expression::StringLiteral(sep)) = arg.as_expression() else {
        return false;
    };
    CSV_SEPARATORS.contains(&sep.value.as_str())
}

/// A CSV context is established when either:
///   - a `text/csv` content-type literal is *emitted* in the join's enclosing
///     function (the response building this CSV is in scope), or
///   - a csv-named binding or enclosing function surrounds this join (the row
///     is assigned to / pushed onto / returned from a csv-named builder).
///
/// Both signals are scoped to this join's lexical neighborhood — never the
/// whole file — so a bare `.join(",")` in an unrelated helper cannot borrow a
/// CSV signal from a different endpoint in the same module.
fn in_csv_context<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    enclosing_scope_emits_text_csv(node, semantic) || has_csv_named_ancestor(node, semantic)
}

/// True when a `text/csv` *content-type string literal* is emitted within the
/// nearest function enclosing this join — i.e. the function produces a CSV
/// response. The signal is an AST `text/csv` string-literal node (not a
/// comment, not a substring of another token) in an *emit position*: a call
/// argument (`res.setHeader("Content-Type", "text/csv")`, `res.type("text/csv")`,
/// `new Response(body, { headers })`) or the value of a `content-type` object
/// property.
///
/// This is an allowlist of emit positions — not a denylist of consumers — so a
/// literal that merely *inspects* an incoming content type (a comparison, a
/// `case`, a stored `const`, a supported-types array, a type alias, a `throw`
/// message) is never mistaken for a CSV emission. With no enclosing function (a
/// top-level join) there is no CSV-emitting scope to borrow from, so this
/// returns false.
fn enclosing_scope_emits_text_csv<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let Some(scope_span) = nodes.ancestors(node.id()).find_map(|a| match a.kind() {
        AstKind::Function(func) => Some(func.span),
        AstKind::ArrowFunctionExpression(arrow) => Some(arrow.span),
        _ => None,
    }) else {
        return false;
    };

    nodes.iter().any(|n| {
        let AstKind::StringLiteral(lit) = n.kind() else {
            return false;
        };
        if lit.value.as_str() != "text/csv" {
            return false;
        }
        if lit.span.start < scope_span.start || lit.span.end > scope_span.end {
            return false;
        }
        is_content_type_emit_position(n, semantic)
    })
}

/// True when a `text/csv` literal sits in a response-emitting position: a
/// call argument (header/response setters take it as a value), or the value
/// of an object property keyed by `content-type` (a `headers`/`ResponseInit`
/// object literal). Every other position — comparison operand, `case` label,
/// variable initializer, array element, type annotation, `throw` argument's
/// inner expression — inspects or stores the type rather than emitting it, and
/// does not establish CSV context.
fn is_content_type_emit_position<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        // `res.setHeader("Content-Type", "text/csv")`, `res.type("text/csv")`,
        // `new Response(body, init)` — the literal is passed into a call.
        AstKind::CallExpression(_) => true,
        // `{ "Content-Type": "text/csv" }` — value of a content-type property.
        AstKind::ObjectProperty(prop) => property_key_is_content_type(&prop.key),
        _ => false,
    }
}

/// True when an object-property key is a `content-type` header name
/// (case-insensitive), whether written as a string literal or an identifier.
fn property_key_is_content_type(key: &oxc_ast::ast::PropertyKey) -> bool {
    use oxc_ast::ast::PropertyKey;
    let name = match key {
        PropertyKey::StringLiteral(s) => s.value.as_str(),
        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
        _ => return false,
    };
    name.eq_ignore_ascii_case("content-type") || name.eq_ignore_ascii_case("contenttype")
}

/// Walks the ancestor chain looking for a name that contains `csv`
/// (case-insensitive): a variable the row is bound to, an assignment target, a
/// method receiver the row is pushed onto, or an enclosing function name.
fn has_csv_named_ancestor<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let named = match ancestor.kind() {
            AstKind::VariableDeclarator(decl) => match &decl.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => name_is_csv(id.name.as_str()),
                _ => false,
            },
            AstKind::Function(func) => {
                func.id.as_ref().is_some_and(|id| name_is_csv(id.name.as_str()))
            }
            AstKind::CallExpression(call) => callee_context_is_csv(call),
            AstKind::AssignmentExpression(assign) => assignment_target_is_csv(&assign.left),
            _ => false,
        };
        if named {
            return true;
        }
    }
    false
}

/// True when a `.push(...)`/`.write(...)` (or any method) receiver, or the
/// callee itself, is csv-named — e.g. `csvRows.push(row.join(","))`.
fn callee_context_is_csv(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(obj) => name_is_csv(obj.name.as_str()),
            _ => false,
        },
        Expression::Identifier(id) => name_is_csv(id.name.as_str()),
        _ => false,
    }
}

/// True when an assignment target is a csv-named identifier or member — e.g.
/// `csvLines += row.join(",")` or `out.csv = row.join(",")`.
fn assignment_target_is_csv(target: &oxc_ast::ast::AssignmentTarget) -> bool {
    use oxc_ast::ast::AssignmentTarget;
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(id) => name_is_csv(id.name.as_str()),
        AssignmentTarget::StaticMemberExpression(member) => name_is_csv(member.property.name.as_str()),
        _ => false,
    }
}

/// True when `name` has a `csv` word token — a case/`_`/`-`/non-alphanumeric
/// boundary on each side. `csvRows`, `buildCsvRow`, and `rows_csv` match;
/// `csvironment` or `recsv` (where `csv` is buried mid-word) do not.
fn name_is_csv(name: &str) -> bool {
    split_identifier_words(name).any(|word| word.eq_ignore_ascii_case("csv"))
}

/// Splits an identifier into word tokens at camelCase boundaries and any
/// non-alphanumeric separator (`_`, `-`, `.`). `buildCsvRow` → `build`, `Csv`,
/// `Row`.
fn split_identifier_words(name: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    let bytes = name.as_bytes();
    let mut boundaries = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        let is_sep = !b.is_ascii_alphanumeric();
        let is_camel_boundary =
            i > 0 && b.is_ascii_uppercase() && bytes[i - 1].is_ascii_lowercase();
        if is_sep {
            if start < i {
                boundaries.push((start, i));
            }
            start = i + 1;
        } else if is_camel_boundary {
            boundaries.push((start, i));
            start = i;
        }
    }
    if start < bytes.len() {
        boundaries.push((start, bytes.len()));
    }
    boundaries.into_iter().map(move |(s, e)| &name[s..e])
}

/// A cell is flagged when it is a *dynamic* string expression not already
/// wrapped in an escape helper. Literals (string/number/boolean/null/regexp/
/// bigint and a substitution-free template) are safe and skipped, as are cells
/// wrapped in a known escape call.
fn is_unescaped_dynamic_cell(element: &ArrayExpressionElement) -> bool {
    let expr = match element {
        // Spreads (`...cells`) and elisions carry no inspectable cell; do not
        // flag — we cannot establish they are dynamic-and-unescaped.
        ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => {
            return false;
        }
        other => other.to_expression(),
    };
    expr_is_dynamic_unescaped(expr)
}

fn expr_is_dynamic_unescaped(expr: &Expression) -> bool {
    match expr {
        // Literal cells are not attacker-controlled.
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_) => false,

        // A template with no substitutions is a literal; with substitutions it
        // is dynamic.
        Expression::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),

        // `escapeCsv(value)` — wrapped in a known escape helper, so safe.
        Expression::CallExpression(call) => !callee_is_escape_helper(&call.callee),

        // Identifiers, member access, conditionals, etc. are dynamic.
        Expression::Identifier(_)
        | Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_) => true,

        // Any other shape (concatenations, parenthesized, logical/conditional
        // wrappers) is not flagged: the cases above cover the common dynamic
        // cells, and silence on the rest keeps the FP rate at zero.
        _ => false,
    }
}

/// True when a call's callee names a known formula-escape helper, by the
/// function identifier or the method name (`x.escapeCsv(...)`).
fn callee_is_escape_helper(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => ESCAPE_HELPERS.contains(&id.name.to_ascii_lowercase().as_str()),
        Expression::StaticMemberExpression(member) => {
            ESCAPE_HELPERS.contains(&member.property.name.to_ascii_lowercase().as_str())
        }
        _ => false,
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // --- Bad: fires inside a CSV context ---

    #[test]
    fn flags_dynamic_cells_in_csv_named_builder() {
        let src = r#"function buildCsvRow(user, gtin) {
  return [user.name, gtin].join(";");
}"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_dynamic_cells_with_text_csv_content_type() {
        let src = r#"function rows(row) {
  res.setHeader("Content-Type", "text/csv");
  return [row.field, row.other].join(",");
}"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_pushed_csv_row() {
        let src = r#"csvRows.push([user.name, user.email].join(";"));"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // A `text/csv` content type in a `ResponseInit` headers object is an emit
    // position — the row built into that response must flag.
    #[test]
    fn flags_response_init_text_csv_headers() {
        let src = r#"function rows(row) {
  return new Response([row.a, row.b].join(","), { headers: { "Content-Type": "text/csv" } });
}"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // --- Good: escaped or literal cells in a CSV context ---

    #[test]
    fn allows_escaped_cells() {
        let src = r#"function buildCsvRow(user, gtin) {
  return [escapeCsv(user.name), escapeCsv(gtin)].join(";");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn allows_literal_header_cells() {
        let src = r#"function buildCsvHeader() {
  return ["Name", "GTIN"].join(";");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // --- CRITICAL guardrail negatives: no CSV context ---

    #[test]
    fn does_not_flag_join_without_csv_context() {
        let src = r#"const label = [a, b].join(",");"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A `text/csv` literal in one endpoint must NOT lend CSV context to a bare
    // join in an unrelated function in the same file. The `text/csv` check is
    // scoped to the join's enclosing function.
    #[test]
    fn does_not_flag_unrelated_join_when_text_csv_elsewhere() {
        let src = r#"export function downloadReport(res) {
  res.setHeader("Content-Type", "text/csv");
}
export function cacheKey(tenant, region) {
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // `csv` must match as a word token, not a substring: a binding whose name
    // merely contains the letters c-s-v (`csvironment`) is not a CSV builder.
    #[test]
    fn does_not_flag_substring_csv_name() {
        let src = r#"const csvironment = [a, b].join(",");"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A `text/csv` in a comment is not an emitted content type — the join must
    // not borrow CSV context from it.
    #[test]
    fn does_not_flag_text_csv_in_comment() {
        let src = r#"function cacheKey(tenant, region) {
  // not text/csv related at all
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A handler that *consumes* CSV (checks an incoming content type) but builds
    // a non-CSV value must not fire — the `text/csv` literal is a consumer check,
    // not an emitted response.
    #[test]
    fn does_not_flag_text_csv_consumer_check() {
        let src = r#"function cacheKey(req, tenant, region) {
  if (req.headers["content-type"] === "text/csv") {
    parseUpload(req);
  }
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // An *indirect* consumer: the `text/csv` literal is bound to a const, then
    // compared. A stored literal is not an emit position, so a non-CSV join in
    // the same handler must not fire.
    #[test]
    fn does_not_flag_indirect_consumer_const() {
        let src = r#"function handle(req, tenant, region) {
  const CSV_MIME = "text/csv";
  if (req.headers["content-type"] !== CSV_MIME) return null;
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Content negotiation over a supported-types array: `text/csv` is an array
    // element (not an emit position), so the join must not fire.
    #[test]
    fn does_not_flag_supported_types_array() {
        let src = r#"function negotiate(accept, tenant, region) {
  const SUPPORTED = ["application/json", "text/csv", "text/plain"];
  if (!SUPPORTED.includes(accept)) return null;
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // `text/csv` as a non-content-type object-property value is not an emit
    // signal for an unrelated join.
    #[test]
    fn does_not_flag_text_csv_as_unrelated_object_value() {
        let src = r#"function build(tenant, region) {
  const types = { report: "text/csv" };
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // `text/csv` as a `switch` case label inspects a format; not an emit.
    #[test]
    fn does_not_flag_text_csv_switch_case() {
        let src = r#"function build(fmt, tenant, region) {
  switch (fmt) {
    case "text/csv":
      handle();
      break;
  }
  return [tenant, region].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn does_not_flag_dynamic_join_in_unrelated_code() {
        let src = r#"function buildPath(segments) {
  return segments.map(s => s.id).join(",");
}
const key = [tenant, region].join(",");"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn does_not_flag_numeric_cells() {
        let src = r#"function buildCsvRow() {
  return [1, 2, 3].join(",");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn does_not_flag_non_csv_separator() {
        // A space-joined row inside a csv-named builder is not a CSV serialization.
        let src = r#"function buildCsvRow(user) {
  return [user.first, user.last].join(" ");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // --- Method-name escape helper is also recognized ---

    #[test]
    fn allows_method_escape_helper() {
        let src = r#"function buildCsvRow(user, gtin) {
  return [csv.escapeCsv(user.name), csv.escapeCsv(gtin)].join(";");
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }
}
