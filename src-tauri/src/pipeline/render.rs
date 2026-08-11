//! Deterministic HTML renderer for `PageBlocks`.
//!
//! Generator models emit a small JSON DSL (see `runtime::types::Block`).
//! This module owns ALL visual decisions: spacing, palette, SVG diagram
//! geometry, callout styles. By keeping rendering on the Rust side we
//! get pixel-stable pages regardless of which model produced the JSON.
//!
//! Hand-edit anything here, never the model output.

use std::fmt::Write;

use anyhow::Context;

use crate::db::Db;
use crate::runtime::{Block, DiagramEdge, DiagramNode, PageBlocks, QuizQuestion};

const RENDERER_VERSION: &str = "4";

/// Render a full HTML document for a page. The SDK is injected later by
/// `pipeline::stages::embed_sdk` (so it stays in one place).
///
/// `page_id` is embedded as a `<meta name="smilie-page-id">` so the SDK
/// running inside the iframe can identify itself when posting messages
/// (e.g. quiz attempts) back up to the host.
pub fn render_page(page_id: &str, fallback_title: &str, blocks: &PageBlocks) -> String {
    let title = if blocks.title.trim().is_empty() {
        fallback_title
    } else {
        blocks.title.as_str()
    };

    let mut body = String::new();
    let _ = writeln!(
        body,
        r#"<header class="smilie-page-head"><h1 class="smilie-page-title">{}</h1></header>"#,
        escape(title)
    );
    body.push_str(r#"<main class="smilie-page-body">"#);
    for block in &blocks.blocks {
        body.push_str(&render_block(block));
    }
    body.push_str("</main>");

    // Tight, valid HTML5 head. `color-scheme: dark` lets the iframe's
    // form controls + scrollbars adopt the dark theme without us hand-
    // styling them. `theme-color` colors the iOS status bar / Android
    // chrome when the page is loaded into the system webview.
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="color-scheme" content="dark">
<meta name="theme-color" content="#07060b">
<meta name="referrer" content="no-referrer">
<meta name="smilie-page-id" content="{page_id_escaped}">
<meta name="smilie-renderer-version" content="{renderer_version}">
<title>{title_escaped}</title>
</head>
<body>
{body}
<script>(function(){{ function fire(){{ document.dispatchEvent(new CustomEvent("smilie:ready")); }} if (window.Smilie) {{ try {{ window.Smilie.applyTheme && window.Smilie.applyTheme(); }} catch (e) {{}} }} fire(); }})();</script>
</body>
</html>
"##,
        page_id_escaped = escape(page_id),
        renderer_version = RENDERER_VERSION,
        title_escaped = escape(title),
        body = body
    )
}

/// Upgrade saved structured pages to the current deterministic renderer.
/// Older free-form pages are left untouched when their source payload cannot
/// be parsed as `PageBlocks`.
pub async fn refresh_saved_pages(db: &Db) -> anyhow::Result<usize> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, title, html_path FROM pages \
         WHERE html_path IS NOT NULL AND html_path <> ''",
    )
    .fetch_all(&db.pool)
    .await?;

    let version_marker =
        format!(r#"<meta name="smilie-renderer-version" content="{RENDERER_VERSION}">"#);
    let mut refreshed = 0;
    for (page_id, title, html_path) in rows {
        let html_path = std::path::PathBuf::from(html_path);
        let current = match std::fs::read_to_string(&html_path) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(path = %html_path.display(), %error, "could not inspect saved page");
                continue;
            }
        };
        if current.contains(&version_marker) {
            continue;
        }

        let Some(parent) = html_path.parent() else {
            continue;
        };
        let source_path = parent.join("source.json");
        let source = match std::fs::read_to_string(&source_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(blocks) = parse_saved_blocks(&source) else {
            continue;
        };
        let upgraded = embed_page_sdk(&render_page(&page_id, &title, &blocks));
        std::fs::write(&html_path, upgraded)
            .with_context(|| format!("upgrading saved page {}", html_path.display()))?;
        refreshed += 1;
    }
    Ok(refreshed)
}

fn parse_saved_blocks(source: &str) -> Option<PageBlocks> {
    serde_json::from_str(source).ok().or_else(|| {
        let start = source.find('{')?;
        let end = source.rfind('}')?;
        serde_json::from_str(&source[start..=end]).ok()
    })
}

fn render_block(block: &Block) -> String {
    match block {
        Block::Heading { level, text } => {
            let level = (*level).clamp(2, 4);
            format!(
                r#"<h{level} class="smilie-h{level}">{}</h{level}>"#,
                escape(text)
            )
        }
        Block::Paragraph { text } => {
            format!(r#"<p class="smilie-p">{}</p>"#, render_prose(text))
        }
        Block::Callout { kind, title, body } => render_callout(kind, title.as_deref(), body),
        Block::KeyTerm { term, definition } => format!(
            r#"<div class="smilie-card smilie-keyterm"><div class="smilie-term">{t}</div><div class="smilie-definition">{d}</div></div>"#,
            t = escape(term),
            d = render_prose(definition),
        ),
        Block::Formula { latex, caption } => render_formula(latex, caption.as_deref()),
        Block::List { items, ordered } => render_list(items, *ordered),
        Block::Quiz { questions } => render_quiz(questions),
        Block::DiagramFlow {
            title,
            nodes,
            edges,
        } => render_diagram_flow(title.as_deref(), nodes, edges),
        Block::SourceRef {
            section_ids,
            page_numbers,
        } => render_source_ref(section_ids, page_numbers),
        Block::Code { lang, code } => render_code(lang.as_deref(), code),
    }
}

fn render_callout(kind: &str, title: Option<&str>, body: &str) -> String {
    let kind_class = match kind {
        "warn" | "warning" => "warn",
        "key" | "important" => "key",
        "note" => "note",
        _ => "info",
    };
    let title_html = match title {
        Some(t) if !t.trim().is_empty() => {
            format!(r#"<div class="smilie-callout-title">{}</div>"#, escape(t))
        }
        _ => String::new(),
    };
    format!(
        r#"<aside class="smilie-callout smilie-callout-{kind_class}">{title_html}<div class="smilie-callout-body">{body_html}</div></aside>"#,
        body_html = render_prose(body)
    )
}

fn render_formula(latex: &str, caption: Option<&str>) -> String {
    let cap = match caption {
        Some(c) if !c.trim().is_empty() => {
            format!(
                r#"<div class="smilie-formula-caption">{}</div>"#,
                render_prose(c)
            )
        }
        _ => String::new(),
    };
    format!(
        r#"<figure class="smilie-formula"><div class="smilie-formula-render" data-latex="{latex}"><code class="smilie-formula-fallback">{latex}</code></div>{cap}</figure>"#,
        latex = escape(latex)
    )
}

fn render_list(items: &[String], ordered: bool) -> String {
    let tag = if ordered { "ol" } else { "ul" };
    let mut out = format!(r#"<{tag} class="smilie-list">"#);
    for item in items {
        let _ = write!(out, r#"<li>{}</li>"#, render_prose(item));
    }
    let _ = write!(out, "</{tag}>");
    out
}

fn render_quiz(questions: &[QuizQuestion]) -> String {
    // Hand the questions to the SDK's interactive primitive. Embed the
    // question data as a JSON island the SDK reads on load.
    let payload = serde_json::to_string(questions).unwrap_or_else(|_| "[]".into());
    let payload_escaped = payload.replace('<', "\\u003c");
    format!(
        r#"<section class="smilie-quiz">
<noscript>Quiz requires JavaScript.</noscript>
<script type="application/json" class="smilie-quiz-data">{payload}</script>
<script>(function(){{
  if(!window.Smilie || !window.Smilie.quiz) return;
  var el = document.currentScript.parentElement;
  var island = el.querySelector(".smilie-quiz-data");
  try {{
    var data = JSON.parse((island && island.textContent) || "[]");
    if (island) island.remove();
    window.Smilie.quiz(el, data);
  }} catch (error) {{
    if (island) island.remove();
    console.error("Smilie quiz data could not be parsed", error);
    var message = document.createElement("p");
    message.className = "smilie-quiz-error";
    message.textContent = "This quiz could not be loaded.";
    el.appendChild(message);
  }}
}})();</script>
</section>"#,
        payload = payload_escaped
    )
}

fn render_source_ref(sections: &[String], pages: &[u32]) -> String {
    let mut chips = String::new();
    for s in sections {
        let _ = write!(chips, r#"<span class="smilie-chip">§ {}</span>"#, escape(s));
    }
    for p in pages {
        let _ = write!(chips, r#"<span class="smilie-chip">page {p}</span>"#);
    }
    format!(r#"<div class="smilie-source-ref">{chips}</div>"#)
}

fn render_code(lang: Option<&str>, code: &str) -> String {
    let lang_class = lang
        .filter(|l| !l.is_empty())
        .map(|l| format!(" lang-{}", escape(l)))
        .unwrap_or_default();
    format!(
        r#"<pre class="smilie-code{lang_class}"><code>{c}</code></pre>"#,
        c = escape(code)
    )
}

/// Responsive semantic flow layout. Fixed-coordinate SVG text cannot wrap,
/// which made realistic model labels overlap at ordinary window widths.
/// HTML nodes keep the same visual language while allowing every label and
/// note to wrap naturally; non-sequential relationships are preserved in a
/// compact relation list below the primary path.
fn render_diagram_flow(
    title: Option<&str>,
    nodes: &[DiagramNode],
    edges: &[DiagramEdge],
) -> String {
    if nodes.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    if let Some(t) = title {
        if !t.trim().is_empty() {
            let _ = write!(
                out,
                r#"<div class="smilie-diagram-title">{}</div>"#,
                escape(t)
            );
        }
    }

    let _ = write!(
        out,
        r#"<figure class="smilie-diagram" role="group" aria-label="flow diagram"><ol class="smilie-flow" style="--smilie-flow-count:{}">"#,
        nodes.len().min(4)
    );

    for (i, node) in nodes.iter().enumerate() {
        let _ = write!(
            out,
            r#"<li class="smilie-flow-step"><div class="smilie-flow-node"><span class="smilie-flow-index">{:02}</span><strong>{}</strong>"#,
            i + 1,
            render_prose(&node.label),
        );
        if let Some(note) = &node.note {
            if !note.trim().is_empty() {
                let _ = write!(
                    out,
                    r#"<span class="smilie-flow-note">{}</span>"#,
                    render_prose(note),
                );
            }
        }
        out.push_str("</div>");

        if let Some(next) = nodes.get(i + 1) {
            let edge = edges
                .iter()
                .find(|edge| edge.from == node.id && edge.to == next.id);
            out.push_str(r#"<div class="smilie-flow-link">"#);
            if let Some(label) = edge.and_then(|edge| edge.label.as_deref()) {
                if !label.trim().is_empty() {
                    let _ = write!(out, r#"<span>{}</span>"#, render_prose(label));
                }
            }
            out.push_str(r#"<b aria-hidden="true">→</b></div>"#);
        }
        out.push_str("</li>");
    }
    out.push_str("</ol>");

    let non_sequential: Vec<&DiagramEdge> = edges
        .iter()
        .filter(|edge| {
            !nodes
                .windows(2)
                .any(|pair| pair[0].id == edge.from && pair[1].id == edge.to)
        })
        .collect();
    if !non_sequential.is_empty() {
        out.push_str(
            r#"<div class="smilie-flow-relations" aria-label="Additional relationships">"#,
        );
        for edge in non_sequential {
            let from = nodes
                .iter()
                .find(|node| node.id == edge.from)
                .map(|node| node.label.as_str())
                .unwrap_or(edge.from.as_str());
            let to = nodes
                .iter()
                .find(|node| node.id == edge.to)
                .map(|node| node.label.as_str())
                .unwrap_or(edge.to.as_str());
            let _ = write!(
                out,
                r#"<span><strong>{}</strong> → <strong>{}</strong>{}</span>"#,
                render_prose(from),
                render_prose(to),
                edge.label
                    .as_deref()
                    .filter(|label| !label.trim().is_empty())
                    .map(|label| format!(": {}", render_prose(label)))
                    .unwrap_or_default(),
            );
        }
        out.push_str("</div>");
    }

    out.push_str("</figure>");
    out
}

/// Smilie Page SDK source — same blob the per-paper pipeline injects
/// into every learning page. Exposed via `embed_page_sdk` so other
/// pipelines (comparisons, level regeneration) can render
/// self-contained pages without duplicating the file include.
const PAGE_SDK: &str = include_str!("../../assets/page-sdk.js");
const TEMML: &str = include_str!("../../assets/temml.min.js");

/// Inline the Smilie Page SDK into already-rendered page HTML.
///
/// The SDK source contains JSDoc that documents `<script>` usage —
/// those comments include the literal text `</script>`. When inlined
/// inside a `<script>` tag, the HTML parser eagerly closes the script
/// at the FIRST `</script>` it sees regardless of JS context, so the
/// rest of the SDK ends up in the document as visible text. Escape the
/// closing tag with a backslash; JS sees no change because backslash
/// escapes inside string literals + comments are no-ops, but the HTML
/// tokenizer no longer recognizes the closer.
pub fn embed_page_sdk(model_html: &str) -> String {
    let safe_temml = TEMML.replace("</script>", "<\\/script>");
    let safe_sdk = PAGE_SDK.replace("</script>", "<\\/script>");
    let sdk_tag =
        format!("<script>{safe_temml}</script><script>{safe_sdk}\nSmilie.applyTheme();</script>");

    if let Some(idx) = model_html.to_lowercase().find("<head>") {
        let insert_at = idx + "<head>".len();
        let mut out = String::with_capacity(model_html.len() + sdk_tag.len());
        out.push_str(&model_html[..insert_at]);
        out.push_str(&sdk_tag);
        out.push_str(&model_html[insert_at..]);
        return out;
    }

    // No <head>: wrap the fragment in a minimal document so the iframe
    // has a valid root + the SDK is always injected.
    format!(
        "<!doctype html><html><head><title>Smilie Page</title>{sdk_tag}</head><body>{model_html}</body></html>"
    )
}

/// HTML-escape for element text. Strict.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Inline content escape that preserves \n as <br>. Use for paragraph/
/// callout body text where the model may have line breaks.
fn escape_inline(s: &str) -> String {
    escape(s).replace('\n', "<br>")
}

/// Models occasionally leak tiny LaTeX fragments into prose fields. Keep
/// formula blocks as real LaTeX, but normalize the common operators that
/// should read as ordinary text in captions, notes, and paragraphs.
fn render_prose(s: &str) -> String {
    let normalized = s
        .replace("^\\circ", "°")
        .replace("\\circ", "°")
        .replace("\\leq", "≤")
        .replace("\\geq", "≥")
        .replace("\\times", "×")
        .replace("\\rightarrow", "→");
    escape_inline(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Block, DiagramEdge, DiagramNode, PageBlocks};

    #[test]
    fn renders_minimal_page() {
        let blocks = PageBlocks {
            title: "Test".into(),
            blocks: vec![Block::Paragraph {
                text: "hi <b>world</b>".into(),
            }],
        };
        let html = render_page("page-123", "fallback", &blocks);
        assert!(html.contains("<title>Test</title>"));
        // Tag escaped
        assert!(html.contains("hi &lt;b&gt;world&lt;/b&gt;"));
        // Page id meta tag is present so the SDK can find it.
        assert!(html.contains(r#"<meta name="smilie-page-id" content="page-123">"#));
    }

    #[test]
    fn diagram_renders_styled_nodes() {
        let blocks = PageBlocks {
            title: "Flow".into(),
            blocks: vec![Block::DiagramFlow {
                title: Some("Concept".into()),
                nodes: vec![
                    DiagramNode {
                        id: "a".into(),
                        label: "Foundation".into(),
                        note: None,
                    },
                    DiagramNode {
                        id: "b".into(),
                        label: "Proxy".into(),
                        note: None,
                    },
                ],
                edges: vec![DiagramEdge {
                    from: "a".into(),
                    to: "b".into(),
                    label: Some("transfer".into()),
                }],
            }],
        };
        let html = render_page("page-x", "title", &blocks);
        assert!(html.contains("smilie-flow-node"));
        assert!(html.contains("smilie-flow-link"));
        // Both labels rendered
        assert!(html.contains("Foundation"));
        assert!(html.contains("Proxy"));
    }

    #[test]
    fn fallback_title_used_when_blocks_title_blank() {
        let blocks = PageBlocks {
            title: "".into(),
            blocks: vec![],
        };
        let html = render_page("page-y", "Fallback Title", &blocks);
        assert!(html.contains("<title>Fallback Title</title>"));
    }

    #[test]
    fn formula_keeps_escaped_source_for_offline_math_rendering() {
        let blocks = PageBlocks {
            title: "Math".into(),
            blocks: vec![Block::Formula {
                latex: r#"x_i = \frac{a}{b} < c \text{ "quoted"}"#.into(),
                caption: Some("Equation 1".into()),
            }],
        };
        let html = render_page("page-math", "Math", &blocks);
        assert!(html.contains("smilie-formula-render"));
        assert!(html.contains("data-latex="));
        assert!(html.contains("&lt; c"));
        assert!(html.contains("&quot;quoted&quot;"));
        assert!(html.contains("Equation 1"));
    }

    #[test]
    fn quiz_json_is_safe_for_quotes_and_script_like_content() {
        let questions = vec![QuizQuestion {
            q: "What's <script>alert('x')</script>?".into(),
            choices: vec!["A's answer".into(), "\"quoted\"".into()],
            answer: 0,
            explanation: Some("It's safe".into()),
        }];
        let html = render_quiz(&questions);
        assert!(html.contains(r#"type="application/json""#));
        assert!(!html.contains("data-smilie-quiz="));
        assert!(!html.contains("<script>alert"));
        let start = html.find(r#"class="smilie-quiz-data">"#).unwrap()
            + r#"class="smilie-quiz-data">"#.len();
        let end = html[start..].find("</script>").unwrap() + start;
        let parsed: Vec<QuizQuestion> = serde_json::from_str(&html[start..end]).unwrap();
        assert_eq!(parsed[0].q, questions[0].q);
    }

    #[test]
    fn prose_math_is_normalized_without_touching_formula_source() {
        assert_eq!(
            render_prose(r#"Pepper: 54.4^\circ, x \leq y"#),
            "Pepper: 54.4°, x ≤ y"
        );
    }

    #[test]
    fn embedded_sdk_contains_offline_math_renderer() {
        let html = embed_page_sdk("<html><head></head><body></body></html>");
        assert!(html.contains("var temml="));
        assert!(html.contains("function renderMath"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn parses_saved_blocks_with_or_without_a_markdown_fence() {
        let raw = r#"{"title":"Saved","blocks":[]}"#;
        assert_eq!(parse_saved_blocks(raw).unwrap().title, "Saved");
        let fenced = format!("```json\n{raw}\n```");
        assert_eq!(parse_saved_blocks(&fenced).unwrap().title, "Saved");
    }
}
