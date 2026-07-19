//! OxcCheck backend for try-catch-new-url.
//!
//! Flags `new URL(...)` not wrapped in a try block. Test files are exempt:
//! they build URLs from controlled fixtures and the framework-under-test's own
//! output, so a throw there surfaces a test bug rather than mishandling of
//! untrusted input.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        use oxc_ast::ast::Expression;

        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Constructor must be `URL`
        let Expression::Identifier(id) = &new_expr.callee else {
            return;
        };
        if id.name.as_str() != "URL" {
            return;
        }

        // Test files build URLs from controlled fixtures and the framework-under-test's
        // own output; a throw there surfaces a test bug, not mishandling of untrusted
        // input, so the defensive try/catch requirement does not apply.
        if crate::rules::path_utils::is_extraneous_test_file(ctx.path) {
            return;
        }

        if is_inside_try_body(node, semantic) {
            return;
        }

        if is_guarded_by_can_parse(new_expr, node, semantic, ctx.source) {
            return;
        }

        if arg_is_trusted(new_expr, semantic) {
            return;
        }

        if base_is_trusted_absolute_url(new_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new URL(...)` throws on invalid input — wrap in try/catch \
                      or gate with `URL.canParse(s)` first."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when the `new URL(arg)` argument is author-controlled and not raw
/// untrusted input: a string literal, a template literal whose every
/// interpolation roots in an env-validated config object (`config.…` / `env.…`),
/// a direct member access rooted in `config` / `env`, a WHATWG `Request.url`
/// getter, the `Location.href` getter for the current page URL, or an identifier
/// whose binding is typed `URL` (or `URL | undefined`).
/// Those cannot fail at runtime in a way the author hasn't already controlled.
fn arg_is_trusted(
    new_expr: &oxc_ast::ast::NewExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    let Some(arg) = new_expr.arguments.first().and_then(|a| a.as_expression()) else {
        return false;
    };
    match arg {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => template_origin_is_trusted(tpl),
        // A WHATWG `Request`'s `.url` getter always returns a well-formed
        // absolute URL, so `new URL(request.url)` cannot throw. Covers
        // `request.url`, `req.url`, and `event.request.url`.
        //
        // `Location.href` is the live getter for the current page URL; the
        // browser only ever assigns it a well-formed absolute URL, so
        // `new URL(location.href)` cannot throw either. Covers `location.href`
        // and `<window|globalThis|document|self>.location.href`.
        Expression::StaticMemberExpression(m)
            if is_request_url_access(m) || is_location_href_access(m) =>
        {
            true
        }
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
            expr_roots_in_trusted_config(arg)
        }
        // Playwright's `Request.url()` method returns the same well-formed
        // absolute URL as the WHATWG `Request.url` property. Covers
        // `request.url()`, `req.url()`, `res.request().url()`, and
        // `event.request().url()`.
        Expression::CallExpression(call) if is_request_url_call(call) => true,
        // `new URL(urlObject)` stringifies an existing `URL` via its `.href`,
        // an already-validated absolute URL, so the constructor cannot throw.
        Expression::Identifier(id) => binding_is_url_typed(id, semantic),
        _ => false,
    }
}

/// True when `ident` resolves to a binding (function parameter or variable
/// declarator) whose explicit TypeScript type annotation is `URL` or a union of
/// `URL` with `undefined`/`null`. The binding is resolved through the symbol
/// table (`reference_id` → symbol → declaration node), so it honours scope and
/// shadowing rather than matching on the name. A `URL` value passed to
/// `new URL(...)` is stringified via `.href`, an already-validated absolute URL,
/// so the constructor cannot throw.
fn binding_is_url_typed(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    let annotation = match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => decl.type_annotation.as_ref(),
        AstKind::FormalParameter(param) => param.type_annotation.as_ref(),
        _ => None,
    };
    annotation.is_some_and(|ann| type_annotation_is_url(&ann.type_annotation))
}

/// True for a `URL` type reference, or a union of `URL` with `undefined`/`null`
/// (`URL | undefined`). The `URL` constructor's parameter is `string | URL`, so
/// a `URL | undefined` value must be narrowed to `URL` before it can reach
/// `new URL(...)` in type-checking code; the declared annotation is then the
/// structural signal that the value is a `URL` object. Any other union member
/// (`URL | string`) leaves the value untrusted.
fn type_annotation_is_url(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    if is_url_reference(ty) {
        return true;
    }
    let TSType::TSUnionType(union) = ty else {
        return false;
    };
    let mut has_url = false;
    for member in &union.types {
        if is_url_reference(member) {
            has_url = true;
        } else if !matches!(member, TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_)) {
            return false;
        }
    }
    has_url
}

/// True for a bare `URL` type reference (`TSTypeReference` named `URL`), not a
/// qualified or generic-applied form.
fn is_url_reference(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = ty else {
        return false;
    };
    matches!(&tref.type_name, TSTypeName::IdentifierReference(id) if id.name == "URL")
}

/// True when the base URL of `new URL(...)` is structurally guaranteed to be a
/// valid absolute URL, so the constructor cannot throw on the base.
///
/// Five trusted base shapes:
/// * `import.meta.url` — the URL the engine assigns when it loads the module.
///   Covers both ESM idioms: single-arg `new URL(import.meta.url)` (the sole
///   argument is the base) and two-arg `new URL(<relative>, import.meta.url)`
///   (the second argument is the base).
/// * a string-literal base that opens with a `scheme://` authority prefix —
///   `new URL(<relative>, 'http://a.com')` resolves against a compile-time
///   constant absolute URL, so the base cannot throw. This is the symmetric
///   two-arg counterpart of the single-arg string-literal trust in
///   `arg_is_trusted`. The scheme must be *leading* (an anchored RFC-3986
///   scheme), not an incidental `://` somewhere in a scheme-less path/query
///   (`'/redirect?to=http://x'`), which is still a relative base and throws.
/// * an `origin` getter that always returns a valid `scheme://host:port`
///   absolute-URL origin: either `Location.origin` (`location.origin`, or
///   `.location.origin` on a standard host object
///   `window`/`globalThis`/`document`/`self`) or the
///   `WindowOrWorkerGlobalScope.origin` getter read directly on a well-known
///   global (`window.origin`/`self.origin`/`globalThis.origin`). Both are
///   spec-defined to serialize a real origin, so resolving a relative reference
///   against either cannot throw on the base. An arbitrary `obj.origin` (which
///   may hold a non-URL value) still flags.
/// * a `Location.href` getter — `location.href`, or `.location.href` on the same
///   standard host objects. The browser only ever assigns `Location.href` a
///   well-formed absolute URL, so resolving a relative reference against it
///   cannot throw on the base. Shares the `is_location_href_access` helper with
///   the single-arg trust in `arg_is_trusted`, so the same `location.href` is
///   trusted in both argument positions.
/// * a `Node.baseURI` getter — a `.baseURI` read on any receiver. The DOM spec
///   defines `Node.baseURI` to always return a valid absolute URL; this mirrors
///   the any-receiver property-shape trust of the template `.protocol` origin.
///
/// A dynamic base (any other second argument — e.g. a plain variable or a
/// non-trusted member access) still flags.
fn base_is_trusted_absolute_url(new_expr: &oxc_ast::ast::NewExpression) -> bool {
    use oxc_ast::ast::Expression;
    let base = match new_expr.arguments.len() {
        1 => new_expr.arguments.first(),
        2 => new_expr.arguments.get(1),
        _ => None,
    };
    let Some(base) = base.and_then(|a| a.as_expression()) else {
        return false;
    };
    if is_import_meta_url(base) {
        return true;
    }
    // A string-literal base opening with a leading `scheme://` authority prefix
    // is a compile-time-constant absolute URL; resolving a relative reference
    // against it cannot throw on the base. The same anchored-scheme test the
    // template-origin path uses, so an incidental `://` inside a scheme-less
    // path/query does not over-trust.
    if matches!(base, Expression::StringLiteral(lit)
        if leading_scheme_authority_prefix(lit.value.as_str()).is_some())
    {
        return true;
    }
    // An `origin` getter always returns a valid absolute-URL origin, a
    // `Location.href` getter always returns a well-formed absolute URL, and a
    // `Node.baseURI` getter always returns a valid absolute URL (all spec), so
    // resolving a relative reference against any of them cannot throw on the
    // base. `.origin` is trusted on a recognized `Location` receiver
    // (`Location.origin`) or read directly on a `window`/`self`/`globalThis`
    // global (the `WindowOrWorkerGlobalScope.origin` getter); `.href` is
    // restricted to `Location` receivers via the same `is_location_href_access`
    // helper `arg_is_trusted` uses for the first argument; `.baseURI` is trusted
    // on any receiver, like the `.protocol` origin shape.
    if let Expression::StaticMemberExpression(member) = base {
        return (member.property.name == "origin"
            && (object_is_location(&member.object) || object_is_global_scope(&member.object)))
            || is_location_href_access(member)
            || member.property.name == "baseURI";
    }
    false
}

/// True for the `import.meta.url` member read: a `.url` access whose object is
/// the `import.meta` meta-property. Matches only `import.meta.url`, not an
/// arbitrary `.url` access.
fn is_import_meta_url(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    member.property.name.as_str() == "url"
        && matches!(&member.object, Expression::MetaProperty(_))
}

/// True for `<request-like>.url` accesses whose receiver is named
/// `request`/`req` directly or via an outer member (`event.request.url`).
fn is_request_url_access(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    use oxc_ast::ast::Expression;
    if member.property.name != "url" {
        return false;
    }
    match &member.object {
        Expression::Identifier(id) => matches!(id.name.as_str(), "request" | "req"),
        Expression::StaticMemberExpression(m) => {
            matches!(m.property.name.as_str(), "request" | "req")
        }
        _ => false,
    }
}

/// True for a `Location.href` read — `location.href`, or a `.location.href`
/// rooted at a standard host object (`window`/`globalThis`/`document`/`self`).
/// The browser only ever assigns `Location.href` a well-formed absolute URL, so
/// `new URL(location.href)` cannot throw. An arbitrary `foo.href` still flags.
fn is_location_href_access(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    member.property.name == "href" && object_is_location(&member.object)
}

/// True for an expression denoting a browser `Location` object — the bare
/// `location` global, or a `.location` read on a standard host object
/// (`window`/`globalThis`/`document`/`self`). Shared by the `Location.href` and
/// `Location.origin` trusts so both anchor on the same receiver shape.
fn object_is_location(object: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match object {
        // `location`
        Expression::Identifier(id) => id.name == "location",
        // `window.location` / `globalThis.location` / `document.location`
        // / `self.location`
        Expression::StaticMemberExpression(inner) => {
            inner.property.name == "location"
                && matches!(&inner.object, Expression::Identifier(host)
                    if matches!(host.name.as_str(), "window" | "globalThis" | "document" | "self"))
        }
        _ => false,
    }
}

/// True for an expression denoting a `WindowOrWorkerGlobalScope` global — the
/// well-known `window` / `self` / `globalThis` identifiers. Its `origin` getter
/// (`WindowOrWorkerGlobalScope.origin`) returns the serialization of the origin,
/// always a well-formed absolute-URL origin (`scheme://host:port`), identical to
/// `Location.origin`, so a base read from it cannot make `new URL(...)` throw.
fn object_is_global_scope(object: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    matches!(object, Expression::Identifier(id)
        if matches!(id.name.as_str(), "window" | "self" | "globalThis"))
}

/// True for a zero-arg `.url()` method call on a request-like receiver:
/// a `request`/`req` identifier (`request.url()`, `req.url()`) or a zero-arg
/// `.request()` call (`res.request().url()`, `event.request().url()`). The
/// gate stays tight so an arbitrary `foo.url()` still flags.
fn is_request_url_call(call: &oxc_ast::ast::CallExpression) -> bool {
    use oxc_ast::ast::Expression;
    if !call.arguments.is_empty() {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name != "url" {
        return false;
    }
    match &member.object {
        Expression::Identifier(id) => matches!(id.name.as_str(), "request" | "req"),
        Expression::CallExpression(inner) => receiver_is_request_call(inner),
        _ => false,
    }
}

/// True for a zero-arg `.request()` method call, the receiver shape of
/// `res.request().url()` / `event.request().url()`.
fn receiver_is_request_call(call: &oxc_ast::ast::CallExpression) -> bool {
    use oxc_ast::ast::Expression;
    if !call.arguments.is_empty() {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name == "request"
}

/// True when the *origin* of a single-arg `new URL(template)` is
/// author-controlled, even if a later path/query/fragment segment interpolates
/// an untrusted value.
///
/// A single-arg `new URL(s)` throws unless `s` is a valid **absolute** URL — it
/// needs a scheme (`https:`, `file:`, …); a scheme-less string (`/api/x`,
/// `//cdn/x`, `?q=v`, ``) throws. So merely "reaching a `/`" proves nothing: the
/// template must establish a real scheme. Once the origin (scheme + authority)
/// is a valid absolute origin, whatever is appended in the path/query/fragment
/// cannot make the constructor throw.
///
/// Three ways a template fixes a trusted absolute origin:
///
/// * **config-rooted origin** — the template starts with an interpolation (its
///   first quasi is empty) that roots in trusted config, so the env-validated
///   value supplies the whole `scheme://authority`
///   (`${config.client.VITE_BASE_URL}/api/v1/${path}` — origin is the config
///   value, `${path}` is in the path), or
/// * **literal scheme** — the first quasi spells out a `scheme://` prefix
///   (`https://api.example.com/${id}` — origin is literal, `${id}` is in the
///   path), or
/// * **`.protocol`-accessor origin** — the template opens with a URL-object
///   `.protocol` getter immediately followed by the literal `//` separator
///   (`${url.protocol}//${url.host}${path}`). Per the WHATWG URL spec a
///   URL/Location object's `.protocol` always returns a syntactically valid
///   scheme (with trailing `:`), so `${x.protocol}//` is guaranteed to form a
///   valid `scheme://` origin.
///
/// For a network scheme, any interpolation reached *inside the authority*
/// (before the path `/`) must itself root in trusted config, or the origin is
/// untrusted and the template still flags. Interpolations after the path `/` are
/// ignored. The `file://` scheme is the exception: it builds a local-filesystem
/// reference (`file://${process.cwd()}/`) of the same author-controlled
/// provenance as the already-trusted `import.meta.url`, not untrusted network
/// input, so a literal `file://` prefix is trusted whatever its authority
/// interpolates.
///
/// Authority-less schemes (`mailto:`, `data:`, `tel:`) are intentionally not
/// whitelisted: they have no `://`, so they match neither branch and keep
/// flagging. That is the conservative direction — flagging a URL that would not
/// throw — and these shapes do not occur in the HTTP-endpoint use case this rule
/// targets. Widen only on a documented need.
fn template_origin_is_trusted(tpl: &oxc_ast::ast::TemplateLiteral) -> bool {
    // A template that opens with a URL-object `.protocol` accessor followed by
    // the `//` authority separator forms a spec-guaranteed valid `scheme://`
    // origin, so the constructor cannot throw on the origin.
    if template_opens_with_protocol_origin(tpl) {
        return true;
    }

    let Some(first_quasi) = tpl.quasis.first() else {
        return false;
    };
    let first_text = first_quasi.value.raw.as_str();

    // The origin must be anchored to a real scheme. Either the leading quasi
    // spells out a literal `scheme://`, or the template opens with a
    // config-rooted interpolation that *is* the origin. A leading quasi that is
    // neither (a scheme-less path, query, fragment, or relative segment) means
    // `new URL` can throw — keep flagging.
    let scheme_prefix = leading_scheme_authority_prefix(first_text);
    let first_text_after_scheme = match scheme_prefix {
        Some((scheme, rest)) => {
            // A literal `file://` scheme builds a local-filesystem reference
            // (`file://${process.cwd()}/`) of the same author-controlled
            // provenance as the already-trusted `import.meta.url` (itself a
            // `file:` URL) — not untrusted network input. Trust it on that
            // provenance rather than demanding a config-rooted authority.
            if scheme == "file" {
                return true;
            }
            rest
        }
        None => {
            let opens_with_trusted_origin = first_text.trim().is_empty()
                && tpl
                    .expressions
                    .first()
                    .is_some_and(expr_roots_in_trusted_config);
            if !opens_with_trusted_origin {
                return false;
            }
            // The config interpolation supplies scheme + authority; the first
            // quasi is empty, so the authority continues into later quasis.
            first_text
        }
    };

    // Origin scheme is established. Walk the authority quasi-by-quasi until the
    // path begins; every interpolation reached before then is still part of the
    // authority and must root in trusted config too. The scheme's `//` has been
    // stripped from the first quasi, so any remaining `/` begins the path.
    for (index, quasi) in tpl.quasis.iter().enumerate() {
        let text = if index == 0 { first_text_after_scheme } else { quasi.value.raw.as_str() };
        if text.contains('/') {
            return true;
        }
        let Some(expr) = tpl.expressions.get(index) else {
            // Final quasi, no further interpolation: the authority was built
            // entirely from the trusted scheme/config parts checked above.
            return true;
        };
        if !expr_roots_in_trusted_config(expr) {
            return false;
        }
    }
    true
}

/// True for a template that opens with a URL-object `.protocol` accessor
/// immediately followed by the literal `//` origin separator —
/// `` `${url.protocol}//${url.host}${path}` ``. Per the WHATWG URL spec a
/// URL/Location object's `.protocol` getter always returns a syntactically
/// valid scheme (with trailing `:`), so `${x.protocol}//` is structurally
/// guaranteed to form a valid `scheme://` origin and `new URL` cannot throw.
///
/// Both parts are required: the `.protocol` accessor must be the very first
/// thing in the template (leading quasi empty) and the quasi right after it
/// must open with `//`. A bare `${x.protocol}` not followed by `//`
/// (e.g. `${x.protocol}foo`) does not establish an origin and still flags.
///
/// The receiver of `.protocol` is intentionally unconstrained: it is the
/// literal `//` separator (not the receiver shape) that supplies the structural
/// `scheme://` guarantee here, unlike the receiver-anchored `request.url` /
/// `location.href` exemptions above.
fn template_opens_with_protocol_origin(tpl: &oxc_ast::ast::TemplateLiteral) -> bool {
    use oxc_ast::ast::Expression;
    // The accessor must be the very first thing in the template, so the leading
    // quasi is empty.
    if !tpl.quasis.first().is_some_and(|q| q.value.raw.trim().is_empty()) {
        return false;
    }
    let first_interp_is_protocol = matches!(
        tpl.expressions.first(),
        Some(Expression::StaticMemberExpression(m)) if m.property.name == "protocol"
    );
    // The quasi immediately after the first interpolation must open with the
    // `//` authority separator.
    first_interp_is_protocol
        && tpl.quasis.get(1).is_some_and(|q| q.value.raw.starts_with("//"))
}

/// If `quasi` opens with a literal `scheme://` authority separator, return the
/// scheme name and the remainder after the `://` (so the remainder's slashes are
/// not mistaken for the path separator); otherwise `None`. Matches a leading
/// `<scheme>://` where `<scheme>` is a non-empty run of scheme characters per
/// RFC 3986 (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`).
///
/// Example: `"https://api.example.com/v1/"` → `Some(("https", "api.example.com/v1/"))`.
/// A scheme-less `"//cdn/x"` or `"/api/x"` returns `None` — those throw in a
/// single-arg `new URL`, so the template stays untrusted.
fn leading_scheme_authority_prefix(quasi: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = quasi.split_once("://")?;
    let mut chars = scheme.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (first_ok && rest_ok).then_some((scheme, rest))
}

/// True when `expr` is a (possibly nested) member access rooted at an
/// identifier named `config` or `env` — the boot-validated config conventions.
fn expr_roots_in_trusted_config(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::StaticMemberExpression(m) => expr_roots_in_trusted_config(&m.object),
        Expression::ComputedMemberExpression(m) => expr_roots_in_trusted_config(&m.object),
        Expression::Identifier(id) => matches!(id.name.as_str(), "config" | "env"),
        _ => false,
    }
}

fn is_inside_try_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TryStatement(_) => return true,
            // Stop at function boundary
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True if the enclosing function's body contains a `URL.canParse(<arg>)`
/// call lexically *before* this `new URL(<arg>)` site.
///
/// Captures all three documented guard shapes with one substring check:
///
/// * `if (!URL.canParse(s)) return; new URL(s)`
/// * `URL.canParse(s) ? new URL(s).x : fallback`
/// * `URL.canParse(s) && new URL(s).x`
///
/// In every case `URL.canParse` appears earlier in source than the
/// `new URL` it guards. The argument-name match keeps the heuristic
/// honest when an unrelated `URL.canParse(other)` lives in the same
/// function.
fn is_guarded_by_can_parse<'a>(
    new_expr: &oxc_ast::ast::NewExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    use oxc_ast::ast::Expression;
    let arg_name = new_expr
        .arguments
        .first()
        .and_then(|a| a.as_expression())
        .and_then(|e| match e {
            Expression::Identifier(id) => Some(id.name.as_str()),
            _ => None,
        });
    let Some(arg) = arg_name else {
        return false;
    };

    let mut func_start: Option<usize> = None;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => {
                func_start = Some(f.span.start as usize);
                break;
            }
            AstKind::ArrowFunctionExpression(a) => {
                func_start = Some(a.span.start as usize);
                break;
            }
            _ => {}
        }
    }
    // Fall back to start of file for top-level `new URL(...)`.
    let start = func_start.unwrap_or(0);
    let end = new_expr.span.start as usize;
    if start >= end {
        return false;
    }
    let prefix = &source[start..end];
    // Match `URL.canParse(<arg>` — the closing `)` may have arbitrary
    // whitespace before it, so we only anchor on the open-paren + arg.
    let needle = format!("URL.canParse({arg}");
    prefix.contains(&needle)
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_bare_new_url() {
        let d = run_on("const u = new URL(input);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "try-catch-new-url");
    }

    #[test]
    fn flags_new_url_in_fn() {
        let d = run_on("function f(s: string) { return new URL(s); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inside_try() {
        assert!(run_on("try { const u = new URL(input); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn allows_other_constructors() {
        assert!(run_on("const u = new MyUrl(input);").is_empty());
    }

    #[test]
    fn allows_can_parse_early_return_guard() {
        // Regression for rbaumier/comply#30 — URL.canParse() early return.
        let src = r#"
            function resolveHost(s: string | undefined): string | null {
                if (s === undefined || !URL.canParse(s)) {
                    return null;
                }
                return new URL(s).host;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_can_parse_ternary() {
        let src = r#"
            function host(s: string): string | null {
                return URL.canParse(s) ? new URL(s).host : null;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_can_parse_short_circuit() {
        let src = r#"
            function host(s: string): string | undefined {
                return URL.canParse(s) && new URL(s).host;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Regression for #285: `new URL(`${config.client.VITE_BASE_URL}/path`)` —
    // the base is an env-validated config constant, not raw user input.
    #[test]
    fn allows_template_rooted_in_config() {
        let src = r#"const url = new URL(`${config.client.VITE_BASE_URL}/api/v1/laboratories/extract`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_plain_string_literal() {
        assert!(run_on(r#"const u = new URL("https://example.com/x");"#).is_empty());
    }

    #[test]
    fn still_flags_template_with_untrusted_interpolation() {
        let src = r#"const u = new URL(`${req.query.target}/x`);"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Regression for #285 (reopened): the live FP at use-export-csv.ts — a
    // template whose origin is the env-validated config value, followed by an
    // untrusted path-segment interpolation. `new URL` only throws on a bad
    // origin; once the origin is valid, the trailing `${path}` lives in the path
    // and cannot throw, so this must not flag.
    #[test]
    fn allows_template_with_trusted_origin_and_untrusted_path() {
        let src = r#"const url = new URL(`${config.client.VITE_BASE_URL}/api/v1/${path}`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A literal absolute origin with an untrusted path interpolation is equally
    // safe — the origin is fixed, the `${id}` is in the path.
    #[test]
    fn allows_template_with_literal_origin_and_untrusted_path() {
        let src = r#"const u = new URL(`https://api.example.com/users/${id}`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #6312: a template whose origin is a URL object's
    // `.protocol` accessor immediately followed by `//` — per the WHATWG URL
    // spec `.protocol` always returns a valid scheme, so `${url.protocol}//…`
    // is a structurally valid absolute origin and `new URL` cannot throw.
    #[test]
    fn allows_template_with_protocol_accessor_origin() {
        let src = r#"const u = new URL(`${redirectURL.protocol}//${redirectURL.host}${uri}`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A bare `${x.protocol}` not followed by the `//` separator does not
    // establish a valid origin, so the constructor can still throw — keep flagging.
    #[test]
    fn still_flags_protocol_accessor_without_double_slash() {
        let src = r#"const u = new URL(`${x.protocol}foo`);"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // An untrusted interpolation *in the authority* (before the path) still
    // controls the origin, so the constructor can throw — this must keep flagging.
    #[test]
    fn still_flags_template_with_untrusted_origin_interpolation() {
        let src = r#"const u = new URL(`https://${userHost}/path`);"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // A config interpolation in the *authority* (after a literal scheme, before
    // the path) is trusted — the origin is fully author-controlled.
    #[test]
    fn allows_template_with_scheme_and_config_authority() {
        let src = r#"const u = new URL(`https://${config.API_HOST}/path`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A scheme-less template throws in a single-arg `new URL` — reaching a `/`
    // does not make it an absolute URL. Must keep flagging even when the only
    // interpolation is a harmless path segment.
    #[test]
    fn still_flags_scheme_less_path_template() {
        let src = r#"const u = new URL(`/api/v1/${path}`);"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // A protocol-relative template (`//host/...`) has no scheme, so a single-arg
    // `new URL` throws — both the literal-host and interpolated-host shapes flag.
    #[test]
    fn still_flags_protocol_relative_template() {
        assert_eq!(run_on(r#"const u = new URL(`//cdn.example.com/${path}`);"#).len(), 1);
        assert_eq!(run_on(r#"const u = new URL(`//${userHost}/path`);"#).len(), 1);
    }

    // A query/fragment-only template has no origin, so `new URL` throws even when
    // the interpolation roots in config — the config value is not the origin.
    #[test]
    fn still_flags_query_only_template_with_trusted_interpolation() {
        assert_eq!(run_on(r#"const u = new URL(`?q=${config.term}`);"#).len(), 1);
        assert_eq!(run_on(r#"const u = new URL(`#${config.frag}`);"#).len(), 1);
    }

    // An empty template (`new URL(``)`) throws; nothing establishes an origin.
    #[test]
    fn still_flags_empty_template() {
        assert_eq!(run_on(r#"const u = new URL(``);"#).len(), 1);
    }

    // A config-rooted origin followed by an untrusted interpolation *still in the
    // authority* (no path `/` yet) is untrusted — the injected value controls the
    // host, so the constructor can throw.
    #[test]
    fn still_flags_untrusted_authority_after_config_origin() {
        let src = r#"const u = new URL(`${config.BASE}${userInjected}/path`);"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // An untrusted interpolation in the userinfo or port of the authority (before
    // the path) controls the origin, so a malformed value can throw — keep
    // flagging both shapes.
    #[test]
    fn still_flags_untrusted_userinfo_or_port_in_authority() {
        assert_eq!(run_on(r#"const u = new URL(`https://${user}@host/x`);"#).len(), 1);
        assert_eq!(run_on(r#"const u = new URL(`https://host:${port}/x`);"#).len(), 1);
    }

    // A literal scheme followed by a config-rooted authority is trusted: scheme,
    // host, and path are all author-controlled.
    #[test]
    fn allows_literal_scheme_with_config_authority_and_path() {
        let src = r#"const u = new URL(`https://${config.API_HOST}/api/${segment}`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_when_can_parse_is_for_different_arg() {
        let src = r#"
            function host(s: string, other: string): string | null {
                if (!URL.canParse(other)) return null;
                return new URL(s).host;
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for rbaumier/comply#30 (adjacent FP): `new URL(config.BASE_URL)` —
    // direct member access on a boot-validated config object is as safe as a
    // string literal or a template rooted in config.
    #[test]
    fn allows_direct_config_member_expression() {
        assert!(run_on("const u = new URL(config.BASE_URL);").is_empty());
        assert!(run_on("const u = new URL(config.client.VITE_API_URL);").is_empty());
        assert!(run_on("const u = new URL(env.API_BASE_URL);").is_empty());
    }

    #[test]
    fn still_flags_non_config_member_expression() {
        assert_eq!(run_on("const u = new URL(process.env.USER_INPUT);").len(), 1);
        assert_eq!(run_on("const u = new URL(req.query.target);").len(), 1);
    }

    // Regression for #541: `request.url` is a WHATWG Request URL — always a
    // valid absolute URL, so the constructor cannot throw.
    #[test]
    fn allows_request_url() {
        assert!(run_on("const u = new URL(request.url);").is_empty());
        assert!(run_on("const u = new URL(req.url);").is_empty());
        assert!(run_on("const u = new URL(event.request.url);").is_empty());
    }

    #[test]
    fn still_flags_arbitrary_member_url() {
        assert_eq!(run_on("const u = new URL(config2.url);").len(), 1);
    }

    // Regression for #3714: `Location.href` is the live getter for the current
    // page URL — the browser only ever assigns it a well-formed absolute URL, so
    // the constructor cannot throw.
    #[test]
    fn allows_location_href() {
        assert!(run_on("const url = new URL(window.location.href);").is_empty());
        assert!(run_on("const url = new URL(globalThis.location.href);").is_empty());
        assert!(run_on("const url = new URL(location.href);").is_empty());
        assert!(run_on("const url = new URL(document.location.href);").is_empty());
        assert!(run_on("const url = new URL(self.location.href);").is_empty());
    }

    #[test]
    fn still_flags_arbitrary_href() {
        // An arbitrary `.href` is not the `Location.href` getter.
        assert_eq!(run_on("const url = new URL(anchor.href);").len(), 1);
    }

    #[test]
    fn still_flags_location_href_on_non_host_object() {
        // `state` is not a standard `location` host object
        // (window/globalThis/document/self).
        assert_eq!(run_on("const url = new URL(state.location.href);").len(), 1);
    }

    // Regression for #1828: Playwright's `res.request().url()` returns the same
    // well-formed absolute URL as the WHATWG `Request.url` property, so the
    // constructor cannot throw.
    #[test]
    fn allows_request_url_method_call() {
        assert!(run_on("const u = new URL(res.request().url());").is_empty());
        assert!(run_on("const u = new URL(event.request().url());").is_empty());
        assert!(run_on("const u = new URL(request.url());").is_empty());
        assert!(run_on("const u = new URL(req.url());").is_empty());
    }

    #[test]
    fn still_flags_arbitrary_url_method_call() {
        // `foo` is neither `request`/`req` nor a `.request()` call.
        assert_eq!(run_on("const u = new URL(foo.url());").len(), 1);
    }

    #[test]
    fn still_flags_plain_variable() {
        assert_eq!(run_on("const u = new URL(someString);").len(), 1);
    }

    // Regression for #3388: `import.meta.url` is the URL the engine assigns when
    // it loads the module — structurally always a valid absolute URL, so the
    // constructor cannot throw.
    #[test]
    fn allows_import_meta_url_single_arg() {
        assert!(run_on("const u = new URL(import.meta.url);").is_empty());
    }

    #[test]
    fn allows_relative_with_import_meta_url_base() {
        assert!(run_on("const u = new URL('../playground-tmp', import.meta.url);").is_empty());
    }

    #[test]
    fn still_flags_dynamic_single_arg() {
        assert_eq!(run_on("const u = new URL(someVar);").len(), 1);
    }

    #[test]
    fn still_flags_dynamic_base() {
        assert_eq!(run_on("const u = new URL(someVar, otherDynamicBase);").len(), 1);
    }

    // Regression for #6216: a string-literal base carrying a `://` scheme is a
    // compile-time-constant absolute URL (VitePress uses `new URL(url, 'http://a.com')`
    // as a URL-parsing utility), so resolution against it cannot throw on the
    // base — the symmetric two-arg counterpart of the single-arg literal trust.
    #[test]
    fn allows_string_literal_absolute_base() {
        assert!(run_on("const u = new URL(url, 'http://a.com');").is_empty());
        assert!(run_on("const u = new URL(url, 'https://api.example.com/base/');").is_empty());
    }

    // A scheme-less string-literal base is not a valid absolute URL, so the
    // constructor can still throw on the base — keep flagging. The check anchors
    // on a *leading* scheme, so an incidental `://` inside a scheme-less path or
    // query does not over-trust either.
    #[test]
    fn still_flags_scheme_less_string_literal_base() {
        assert_eq!(run_on("const u = new URL(url, '/relative/base');").len(), 1);
        assert_eq!(run_on("const u = new URL(url, '/redirect?to=http://x');").len(), 1);
    }

    // A non-literal base (a variable that is not `import.meta.url`) does not
    // statically establish a valid absolute base — keep flagging.
    #[test]
    fn still_flags_variable_base() {
        assert_eq!(run_on("const u = new URL(x, someVariable);").len(), 1);
    }

    // Regression for #6217: `Location.origin` is a DOM getter that always returns
    // a valid `scheme://host:port` absolute-URL origin, so resolving a relative
    // reference against it cannot throw on the base. Covers the bare `location`
    // global and the standard host-object receivers.
    #[test]
    fn allows_location_origin_base() {
        assert!(run_on("const u = new URL(href, location.origin);").is_empty());
        assert!(run_on("const u = new URL(x, window.location.origin);").is_empty());
        assert!(run_on("const u = new URL(x, globalThis.location.origin);").is_empty());
        assert!(run_on("const u = new URL(x, document.location.origin);").is_empty());
        assert!(run_on("const u = new URL(x, self.location.origin);").is_empty());
    }

    // Regression for #6217: `Node.baseURI` is a DOM getter spec-defined to always
    // return a valid absolute URL on any receiver, so it is a safe base.
    #[test]
    fn allows_base_uri_base() {
        assert!(run_on("const u = new URL(x, link.baseURI);").is_empty());
        assert!(run_on("const u = new URL(x, element.baseURI);").is_empty());
    }

    // `.origin` on a receiver that is not a recognized `Location` object may hold
    // any value (not necessarily a valid URL), so the base can still throw.
    #[test]
    fn still_flags_origin_on_non_location_object() {
        assert_eq!(run_on("const u = new URL(x, someArbitraryObj.origin);").len(), 1);
    }

    // Regression for #7250: `WindowOrWorkerGlobalScope.origin` — `origin` read
    // directly on a `window`/`self`/`globalThis` global — is a spec getter that
    // returns the serialization of the origin, always a well-formed absolute-URL
    // origin, so resolving a relative reference against it cannot throw on the base.
    #[test]
    fn allows_global_scope_origin_base() {
        assert!(
            run_on("const u = new URL(location.pathname + location.search, window.origin);")
                .is_empty()
        );
        assert!(run_on("const u = new URL(page, self.origin);").is_empty());
        assert!(run_on("const u = new URL(page, globalThis.origin);").is_empty());
    }

    // Regression for #6899: `Location.href` always returns a well-formed absolute
    // URL, so it is a valid base argument too — symmetric with the single-arg
    // `new URL(location.href)` trust. Covers the bare `location` global and the
    // standard host-object receivers.
    #[test]
    fn allows_location_href_base() {
        assert!(run_on("const u = new URL(someRelative, location.href);").is_empty());
        assert!(run_on("const u = new URL(x, window.location.href);").is_empty());
        assert!(run_on("const u = new URL(x, document.location.href);").is_empty());
        assert!(run_on("const u = new URL(x, globalThis.location.href);").is_empty());
        assert!(run_on("const u = new URL(x, self.location.href);").is_empty());
    }

    // `.href` on a receiver that is not a recognized `Location` object is not the
    // `Location.href` getter, so the base can still throw — keep flagging.
    #[test]
    fn still_flags_href_base_on_non_location_object() {
        assert_eq!(run_on("const u = new URL(x, anchor.href);").len(), 1);
        assert_eq!(run_on("const u = new URL(x, state.location.href);").len(), 1);
    }

    // An arbitrary member-access base that is not a recognized DOM absolute-URL
    // getter does not statically establish a valid base — keep flagging.
    #[test]
    fn still_flags_arbitrary_member_base() {
        assert_eq!(run_on("const u = new URL(x, config.baseUrl);").len(), 1);
    }

    #[test]
    fn still_flags_arbitrary_dot_url_object() {
        // `meta.url` is a plain member read, not the `import.meta` meta-property.
        assert_eq!(run_on("const u = new URL(meta.url);").len(), 1);
    }

    // Regression for #6110: `new URL(`file://${process.cwd()}/`)` builds a
    // local-filesystem URL from a literal `file://` scheme — the same trust
    // category as `import.meta.url` (itself a `file:` URL). `process.cwd()` is an
    // author-controlled absolute local path, never untrusted network input, so
    // the constructor cannot throw and wrapping it in try/catch would be dead
    // code.
    #[test]
    fn allows_file_scheme_template_with_cwd() {
        assert!(run_on("const CWD = new URL(`file://${process.cwd()}/`);").is_empty());
    }

    // The two-arg form `new URL(`file://...`, base)` is equally safe: the first
    // argument already carries the absolute `file://` origin, so the base is
    // never consulted.
    #[test]
    fn allows_file_scheme_template_with_cwd_and_base() {
        assert!(run_on("const u = new URL(`file://${process.cwd()}/`, base);").is_empty());
    }

    // The `file://` exemption is provenance-based, like `import.meta.url`: a
    // literal `file://` prefix is trusted whatever its authority interpolates,
    // by deliberate design. Pin that contract so a future tightening of the
    // file-scheme authority is a conscious change, not a silent regression.
    #[test]
    fn allows_file_scheme_with_arbitrary_authority() {
        assert!(run_on("const u = new URL(`file://${anything}/p`);").is_empty());
    }

    // The exemption is scoped to the `file://` scheme: a genuinely dynamic
    // argument with no static scheme still has no trusted origin and must keep
    // flagging.
    #[test]
    fn still_flags_dynamic_arg_with_no_static_scheme() {
        assert_eq!(run_on("const u = new URL(userInput);").len(), 1);
    }

    // Regression for #4392: a redirect-asserting test builds a URL from the
    // framework-under-test's own `Location` header; a throw there is a test
    // bug, not untrusted input, so the test file is exempt.
    #[test]
    fn allows_untrusted_new_url_in_test_file() {
        let d = run_at("const loc = new URL(resp.headers.get('location')!);", "index.test.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_untrusted_new_url_in_test_dir() {
        let d = run_at("const loc = new URL(resp.headers.get('location')!);", "src/tests/foo.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn still_flags_untrusted_new_url_in_production_file() {
        let d = run_at("const loc = new URL(resp.headers.get('location')!);", "src/app.ts");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // Regression for #6582: a parameter typed `: URL` already holds a valid URL
    // object; `new URL(urlObject)` stringifies its `.href` and cannot throw.
    #[test]
    fn allows_url_typed_parameter() {
        let src = r#"
            const f = (fileUrl: URL) => {
                const cleanFileUrl = new URL(fileUrl);
                return cleanFileUrl;
            };
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A `URL | undefined`-typed binding carries the `URL` structural signal:
    // under the type system it must be narrowed to `URL` before reaching
    // `new URL(...)`, so the value is a URL object and the constructor cannot throw.
    #[test]
    fn allows_url_or_undefined_typed_variable() {
        let src = r#"
            function f(maybe: URL | undefined) {
                const u: URL | undefined = maybe;
                return new URL(u);
            }
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A `: string`-annotated binding is the common throw case the rule targets —
    // still flagged.
    #[test]
    fn still_flags_string_typed_parameter() {
        let src = r#"
            function f(s: string) {
                return new URL(s);
            }
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // An unannotated binding gives no `: URL` signal — still flagged.
    #[test]
    fn still_flags_unannotated_variable() {
        let src = r#"
            function f(input) {
                const x = input;
                return new URL(x);
            }
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // A union mixing `URL` with a non-nullish member is not provably a `URL`
    // value — still flagged.
    #[test]
    fn still_flags_url_or_string_union() {
        let src = r#"
            function f(u: URL | string) {
                return new URL(u);
            }
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
