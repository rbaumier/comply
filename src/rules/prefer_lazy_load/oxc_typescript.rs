use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::path::Path;
use std::sync::Arc;

pub struct Check;

/// True when the file produces a static image on the server rather than browser
/// markup, so `loading="lazy"` would be meaningless or break the renderer.
///
/// Three file-local signals, any of which exempts every `<img>`/`<iframe>` in
/// the file:
/// - `ImageResponse` appears in the source (Next.js `@vercel/og` /
///   Deno `og_edge` — JSX is rendered to a PNG via Satori).
/// - The file imports from a Deno URL specifier (`https://…` or `jsr:…`); such
///   modules are edge/server bundles consumed by Satori, never shipped to a
///   browser. This catches the child components a handler renders.
/// - The basename matches a Next.js metadata image route convention
///   (`opengraph-image`, `twitter-image`, `icon`, `apple-icon`), or the file
///   lives under a `supabase/functions/` Edge Function directory.
fn is_server_side_image_render(ctx: &CheckCtx) -> bool {
    if ctx.source_contains("ImageResponse") || imports_from_deno_url(ctx.source) {
        return true;
    }
    is_metadata_image_route(ctx.path) || in_supabase_functions_dir(ctx.path)
}

/// True when `source` has an ES import whose specifier is a Deno URL: a
/// `https://` or `jsr:` scheme right after the `from '`/`from "` of an import.
fn imports_from_deno_url(source: &str) -> bool {
    for marker in ["from '", "from \""] {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(marker) {
            let spec_start = from + rel + marker.len();
            let spec = &source[spec_start..];
            if spec.starts_with("https://") || spec.starts_with("jsr:") {
                return true;
            }
            from = spec_start;
        }
    }
    false
}

/// True when the basename (without extension) is a Next.js metadata image route
/// file. These files export JSX that the framework renders to a static image.
fn is_metadata_image_route(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let stem = name.split('.').next().unwrap_or(name);
    let stem = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(stem, "opengraph-image" | "twitter-image" | "icon" | "apple-icon")
}

/// True when the file lives under a `supabase/functions/` Edge Function
/// directory, where JSX is rendered to an OG image rather than a browser DOM.
fn in_supabase_functions_dir(path: &Path) -> bool {
    let mut segments = path.iter().filter_map(|s| s.to_str());
    while let Some(seg) = segments.next() {
        if seg == "supabase" && segments.next() == Some("functions") {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag != "img" && tag != "iframe" {
            return;
        }

        let has_loading = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            name_ident.name.as_str() == "loading"
        });
        if has_loading {
            return;
        }
        if is_server_side_image_render(ctx) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`<{tag}>` should set `loading=\"lazy\"` to defer off-screen loads."),
            severity: Severity::Warning,
            span: None,
        });
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
        path: &Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule;

    #[test]
    fn flags_img_without_loading() {
        assert_eq!(run_rule(&Check, r#"const x = <img src="x.png" />;"#, "t.tsx").len(), 1);
    }

    #[test]
    fn allows_img_with_lazy() {
        assert!(run_rule(&Check, r#"const x = <img src="x.png" loading="lazy" />;"#, "t.tsx").is_empty());
    }

    #[test]
    fn flags_client_img_in_supabase_app_dir() {
        // An ordinary client-rendered `<img>` outside the edge-function dir is
        // still flagged — the exemption is scoped, not project-wide.
        let src = r#"export const Hero = () => <img src="x.png" />;"#;
        assert_eq!(run_rule(&Check, src, "supabase/components/Hero.tsx").len(), 1);
    }

    #[test]
    fn exempts_image_response_handler() {
        let src = r#"
            import { ImageResponse } from 'https://deno.land/x/og_edge@0.0.4/mod.ts'
            export default () => new ImageResponse(<img src="x.png" />)
        "#;
        assert!(run_rule(&Check, src, "handler.tsx").is_empty());
    }

    #[test]
    fn exempts_deno_url_imported_component() {
        // Reproduces issue #1785: the child component a Satori handler renders.
        // It imports React from a Deno URL and does not itself reference
        // `ImageResponse`, yet both `<img>` elements must be exempt.
        let src = r#"
            import React from 'https://esm.sh/react@18.2.0?deno-std=0.140.0'

            export default function CustomerStories({ title, customer }: Props) {
              return (
                <div style={{ display: 'flex' }}>
                  <img src={supabaseLogoUrl} width="90px" height="90px" />
                  {customer && (
                    <img
                      src={imageUrl}
                      style={{ objectFit: 'contain' }}
                    />
                  )}
                </div>
              )
            }
        "#;
        assert!(
            run_rule(&Check, src, "supabase/functions/og-images/component/CustomerStories.tsx")
                .is_empty()
        );
    }

    #[test]
    fn exempts_nextjs_opengraph_image_route() {
        let src = r#"export default function Image() { return <img src="x.png" />; }"#;
        assert!(run_rule(&Check, src, "app/blog/opengraph-image.tsx").is_empty());
    }

    #[test]
    fn exempts_supabase_functions_dir() {
        let src = r#"export const C = () => <img src="x.png" />;"#;
        assert!(run_rule(&Check, src, "supabase/functions/og/Card.tsx").is_empty());
    }
}
