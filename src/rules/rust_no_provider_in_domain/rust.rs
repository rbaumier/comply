//! rust-no-provider-in-domain backend.
//!
//! Two gates, both configurable, both required before anything is reported:
//!
//! 1. **Path** — the file matches one of `domain_globs` (default
//!    `**/domain/**`). Outside the domain, naming the vendor is correct: an
//!    adapter's whole job is to speak one provider's protocol.
//! 2. **Name** — a *declared* identifier carries a `providers` entry as a
//!    whole word segment. Only declaration sites are read (struct, enum,
//!    enum variant, union, trait, type alias, module, function, field,
//!    `let`/`const`/`static` binding), so a provider named in a comment, a
//!    string literal, or an imported adapter type is never flagged — the
//!    rule is about the vocabulary the domain *defines*, not about what it
//!    mentions.
//!
//! Matching splits the identifier at `_`, `-`, and camelCase boundaries, then
//! compares whole segments — `stripe_customer_id` and `StripeCustomerId` both
//! match `stripe`, while `striped_rows` and `pinstripe` do not. Consecutive
//! segments are also joined before comparing, so the acronym casing
//! `OpenAIClient` (`open` + `ai`) still matches `openai`.
//!
//! `meta` is deliberately absent from the default `providers` list:
//! `metadata`, `meta_key` and `EventMeta` are ordinary domain words, and the
//! company of the same name is not worth that much noise.
//!
//! One shape is flagged on purpose even though it can be deliberate: an enum
//! that enumerates the providers themselves (`enum Billing { Stripe, … }`).
//! Whether the business genuinely distinguishes vendors, or the domain is
//! leaking the adapter's catalogue, is a judgement no AST carries — so the
//! rule reports it and the answer is a `// comply-ignore` with the reason.


use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// A `let` binding exposes its identifier under `pattern`; every other kind
/// below carries it under `name`.
const LET_KIND: &str = "let_declaration";

/// Every declaration kind the walker is handed.
const KINDS: &[&str] = &[
    "struct_item",
    "enum_item",
    "enum_variant",
    "union_item",
    "trait_item",
    "type_item",
    "mod_item",
    "function_item",
    "field_declaration",
    "const_item",
    "static_item",
    LET_KIND,
];

/// Per-file verdict, resolved once on the first visited node.
#[derive(Debug)]
enum FileScope {
    /// The path matches no `domain_globs` entry — the rule does not apply.
    Outside,
    /// The path is a domain file; these are the provider names to reject.
    Domain(Vec<String>),
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    /// Memoizes the path-glob verdict and the provider list, both of which are
    /// file-constant but would otherwise be recomputed on every declaration.
    /// `None` = not yet resolved.
    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(None::<FileScope>))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(name_node) = declared_name_node(node) else {
            return;
        };
        let Ok(name) = name_node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let Some(provider) = provider_named_by(name, ctx, state) else {
            return;
        };
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &name_node,
            super::META.id,
            format!(
                "`{name}` names the provider `{provider}` inside the domain. \
                 The domain stays provider-agnostic — name it after the role \
                 it plays (`provisioned_number_reference`), and let the \
                 adapter be the only place that says `{provider}`."
            ),
            Severity::Error,
        ));
    }
}

/// The configured provider `name` carries, or `None` when the file is outside
/// the domain globs or the name is clean.
///
/// The per-file verdict — path gate plus provider list — is memoized in
/// `state`, so the config read and the glob compile happen once per file
/// instead of once per declaration. Matching runs inside that borrow and only
/// the winning provider is copied out, so a clean declaration allocates
/// nothing.
fn provider_named_by(
    name: &str,
    ctx: &CheckCtx,
    state: Option<&mut dyn std::any::Any>,
) -> Option<String> {
    // No memo (a caller outside the standard walk): resolve inline.
    let Some(slot) = state.and_then(|s| s.downcast_mut::<Option<FileScope>>()) else {
        return matched_provider(name, &resolve_scope(ctx)?).map(str::to_owned);
    };
    let scope = slot.get_or_insert_with(|| match resolve_scope(ctx) {
        Some(providers) => FileScope::Domain(providers),
        None => FileScope::Outside,
    });
    let FileScope::Domain(providers) = scope else {
        return None;
    };
    matched_provider(name, providers).map(str::to_owned)
}

/// Read both config keys and apply the path gate. `None` = file is not domain
/// code, or no provider is configured, so there is nothing to report.
fn resolve_scope(ctx: &CheckCtx) -> Option<Vec<String>> {
    let domain_globs = ctx.config.string_list(super::META.id, "domain_globs", ctx.lang);
    if !crate::rules::path_utils::matches_any_glob(ctx.path, &domain_globs) {
        return None;
    }
    let providers: Vec<String> = ctx
        .config
        .string_list(super::META.id, "providers", ctx.lang)
        .into_iter()
        .map(|p| p.to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if providers.is_empty() {
        return None;
    }
    Some(providers)
}

/// The node holding the declared identifier, or `None` for a declaration whose
/// binding is not a plain name (a destructuring `let`, a tuple-struct field).
fn declared_name_node(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() != LET_KIND {
        return node.child_by_field_name("name");
    }
    let pattern = node.child_by_field_name("pattern")?;
    match pattern.kind() {
        "identifier" => Some(pattern),
        // `let mut x = …` wraps the identifier one level down.
        "mut_pattern" => pattern.named_child(0).filter(|n| n.kind() == "identifier"),
        // A tuple / struct / slice pattern binds several names at once; naming
        // one of them after a provider is a weaker signal than a declaration,
        // and reporting the whole pattern would point at the wrong span.
        _ => None,
    }
}

/// The first configured provider carried by `name` as a whole word segment,
/// or `None`.
fn matched_provider<'a>(name: &str, providers: &'a [String]) -> Option<&'a str> {
    let segments = segments(name);
    providers
        .iter()
        .find(|provider| segments_contain(&segments, provider))
        .map(String::as_str)
}

/// True when `provider` equals one segment, or the concatenation of a run of
/// consecutive segments. The run form is what makes the acronym casing
/// `OpenAIClient` (segments `open`, `ai`, `client`) match `openai`.
fn segments_contain(segments: &[String], provider: &str) -> bool {
    for start in 0..segments.len() {
        let mut joined = String::new();
        for segment in &segments[start..] {
            joined.push_str(segment);
            if joined.len() > provider.len() {
                break;
            }
            if joined == provider {
                return true;
            }
        }
    }
    false
}

/// Split an identifier into lowercase word segments at `_`, `-`, and camelCase
/// boundaries: `twilio_phone_sid` → `twilio`, `phone`, `sid`; `StripeClient` →
/// `stripe`, `client`; `OpenAIClient` → `open`, `ai`, `client`.
///
/// An uppercase run stays one segment (`SID` → `sid`) and only breaks before
/// its last letter when a lowercase letter follows, which is the boundary
/// between an acronym and the word after it (`AIClient` → `ai`, `client`).
fn segments(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    for (index, &character) in chars.iter().enumerate() {
        if !character.is_alphanumeric() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if character.is_uppercase() && !current.is_empty() {
            let previous = chars[index - 1];
            let starts_word = previous.is_lowercase() || previous.is_numeric();
            let ends_acronym = previous.is_uppercase()
                && chars.get(index + 1).is_some_and(|next| next.is_lowercase());
            if starts_word || ends_acronym {
                out.push(std::mem::take(&mut current));
            }
        }
        current.extend(character.to_lowercase());
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOMAIN_PATH: &str = "src/domain/phone_number.rs";
    const ADAPTER_PATH: &str = "src/adapters/twilio/client.rs";

    fn run_in_domain(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, DOMAIN_PATH)
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_provider_named_struct_field() {
        // The natalia !115 review finding: `twilio_phone_sid` on a domain entity.
        let d = run_in_domain("struct PhoneNumber { twilio_phone_sid: String }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "rust-no-provider-in-domain");
        assert!(d[0].message.contains("twilio"));
    }

    #[test]
    fn flags_provider_named_struct() {
        assert_eq!(run_in_domain("struct TwilioNumber { id: String }").len(), 1);
    }

    #[test]
    fn flags_provider_named_function() {
        assert_eq!(run_in_domain("fn release_stripe_customer() {}").len(), 1);
    }

    #[test]
    fn flags_provider_named_module() {
        assert_eq!(run_in_domain("mod brevo_contacts {}").len(), 1);
    }

    #[test]
    fn flags_provider_named_enum_variant() {
        let d = run_in_domain("enum Channel { Sms, GladiaTranscript }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("gladia"));
    }

    #[test]
    fn flags_provider_named_trait() {
        assert_eq!(run_in_domain("trait FerriskeyPort {}").len(), 1);
    }

    #[test]
    fn flags_provider_named_type_alias() {
        assert_eq!(run_in_domain("type ScalewayBucket = String;").len(), 1);
    }

    #[test]
    fn flags_provider_named_const() {
        assert_eq!(run_in_domain("const ANTHROPIC_MODEL: &str = \"x\";").len(), 1);
    }

    #[test]
    fn flags_provider_named_let_binding() {
        let src = "fn book() { let twilio_sid = lookup(); }";
        assert_eq!(run_in_domain(src).len(), 1);
    }

    #[test]
    fn flags_provider_named_mut_let_binding() {
        let src = "fn book() { let mut stripe_customer = None; }";
        assert_eq!(run_in_domain(src).len(), 1);
    }

    #[test]
    fn flags_acronym_cased_provider() {
        // `OpenAIPrompt` splits to `open`/`ai`/`prompt`; the run `open`+`ai`
        // reconstructs the configured `openai`.
        assert_eq!(run_in_domain("struct OpenAIPrompt { text: String }").len(), 1);
    }

    #[test]
    fn does_not_flag_outside_domain_paths() {
        // Naming the vendor is the adapter's job — same declarations, no finding.
        let src = "struct TwilioNumber { twilio_phone_sid: String }";
        assert!(run_at(src, ADAPTER_PATH).is_empty());
        assert!(run_at(src, "src/infra/stripe_gateway.rs").is_empty());
        assert!(run_at(src, "src/main.rs").is_empty());
    }

    #[test]
    fn flags_domain_dir_at_any_depth() {
        let src = "struct TwilioNumber { id: String }";
        assert_eq!(run_at(src, "crates/core/src/domain/phone.rs").len(), 1);
        assert_eq!(run_at(src, "./src/domain/telephony/phone.rs").len(), 1);
    }

    #[test]
    fn does_not_flag_provider_in_a_comment() {
        let src = "// Provisioned through twilio, see the adapter.\n\
                   /// Stripe is the current billing backend.\n\
                   struct PhoneNumber { reference: String }";
        assert!(run_in_domain(src).is_empty());
    }

    #[test]
    fn does_not_flag_provider_in_a_string_literal() {
        let src = "fn provider_name() -> &'static str { \"twilio\" }";
        assert!(run_in_domain(src).is_empty());
    }

    #[test]
    fn does_not_flag_a_provider_named_type_that_is_only_referenced() {
        // The domain declares nothing vendor-shaped; the adapter type it takes
        // is a reference, not a declaration.
        let src = "fn provision(client: &TwilioClient) -> Reference { client.reference() }";
        assert!(run_in_domain(src).is_empty());
    }

    #[test]
    fn does_not_flag_meta_by_default() {
        // `meta` is not a default provider: metadata is an ordinary domain word.
        let src = "struct Event { meta: Metadata, meta_key: String }";
        assert!(run_in_domain(src).is_empty());
    }

    #[test]
    fn does_not_flag_a_provider_name_embedded_in_a_longer_word() {
        // Segment equality, not substring: `stripe` must stand on its own.
        let src = "struct Table { striped_rows: bool, pinstripe: u8 }";
        assert!(run_in_domain(src).is_empty());
    }

    #[test]
    fn does_not_flag_role_named_declarations() {
        let src = "struct PhoneNumber { provisioned_number_reference: String }";
        assert!(run_in_domain(src).is_empty());
    }

    #[test]
    fn splits_camel_case_and_acronyms() {
        assert_eq!(segments("twilio_phone_sid"), ["twilio", "phone", "sid"]);
        assert_eq!(segments("TwilioClient"), ["twilio", "client"]);
        assert_eq!(segments("OpenAIClient"), ["open", "ai", "client"]);
        assert_eq!(segments("TWILIO_ACCOUNT_SID"), ["twilio", "account", "sid"]);
    }
}
