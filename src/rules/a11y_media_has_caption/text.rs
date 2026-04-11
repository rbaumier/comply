use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let lower = line.to_lowercase();
            if !lower.contains("<video") && !lower.contains("<audio") {
                continue;
            }
            // Check within 5 lines for <track with kind="captions"
            let end = (idx + 6).min(lines.len());
            let window = &lines[idx..end];
            let has_caption_track = window.iter().any(|l| {
                let ll = l.to_lowercase();
                ll.contains("<track") && ll.contains("kind=\"captions\"")
            });
            if !has_caption_track {
                let element = if lower.contains("<video") {
                    "video"
                } else {
                    "audio"
                };
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-media-has-caption".into(),
                    message: format!(
                        "`<{element}>` elements must have a `<track kind=\"captions\">` child for accessibility."
                    ),
                    severity: Severity::Warning,
                });
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
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    #[test]
    fn flags_video_without_track() {
        let d = run("<video src=\"movie.mp4\"></video>");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("video"));
    }

    #[test]
    fn allows_video_with_caption_track() {
        let source = r#"<video src="movie.mp4">
  <track kind="captions" src="captions.vtt" />
</video>"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_audio_without_track() {
        let d = run("<audio src=\"song.mp3\"></audio>");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("audio"));
    }
}
