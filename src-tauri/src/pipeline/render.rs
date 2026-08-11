//! Deterministic HTML renderer for `PageBlocks`.
//!
//! Generator models emit a small JSON DSL (see `runtime::types::Block`).
//! This module owns ALL visual decisions: spacing, palette, SVG diagram
//! geometry, callout styles. By keeping rendering on the Rust side we
//! get pixel-stable pages regardless of which model produced the JSON.
//!
//! Hand-edit anything here, never the model output.

use std::fmt::Write;

use crate::runtime::{Block, DiagramEdge, DiagramNode, PageBlocks, QuizQuestion};

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
<title>{title_escaped}</title>
</head>
<body>
{body}
<script>(function(){{ function fire(){{ document.dispatchEvent(new CustomEvent("smilie:ready")); }} if (window.Smilie) {{ try {{ window.Smilie.applyTheme && window.Smilie.applyTheme(); }} catch (e) {{}} }} fire(); }})();</script>
</body>
</html>
"##,
        page_id_escaped = escape(page_id),
        title_escaped = escape(title),
        body = body
    )
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
            format!(r#"<p class="smilie-p">{}</p>"#, escape_inline(text))
        }
        Block::Callout { kind, title, body } => render_callout(kind, title.as_deref(), body),
        Block::KeyTerm { term, definition } => format!(
            r#"<div class="smilie-card smilie-keyterm"><div class="smilie-term">{t}</div><div class="smilie-definition">{d}</div></div>"#,
            t = escape(term),
            d = escape_inline(definition),
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
        body_html = escape_inline(body)
    )
}

fn render_formula(latex: &str, caption: Option<&str>) -> String {
    let cap = match caption {
        Some(c) if !c.trim().is_empty() => {
            format!(r#"<div class="smilie-formula-caption">{}</div>"#, escape(c))
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
        let _ = write!(out, r#"<li>{}</li>"#, escape_inline(item));
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
        r#"<section class="smilie-quiz" data-smilie-quiz='{payload}'>
<noscript>Quiz requires JavaScript.</noscript>
<script>(function(){{
  if(!window.Smilie || !window.Smilie.quiz) return;
  var el = document.currentScript.parentElement;
  var data = JSON.parse(el.getAttribute("data-smilie-quiz") || "[]");
  el.removeAttribute("data-smilie-quiz");
  window.Smilie.quiz(el, data);
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

/// Diagram layout: lay nodes out left-to-right at fixed pitch. We control
/// every fill/stroke so unfilled rects can never render as black voids
/// like the model used to produce.
fn render_diagram_flow(
    title: Option<&str>,
    nodes: &[DiagramNode],
    edges: &[DiagramEdge],
) -> String {
    if nodes.is_empty() {
        return String::new();
    }

    const NODE_W: i32 = 160;
    const NODE_H: i32 = 64;
    const GAP_X: i32 = 56;
    const PAD: i32 = 24;
    const STROKE_W: i32 = 2;

    let n = nodes.len() as i32;
    let inner_w = n * NODE_W + (n - 1).max(0) * GAP_X;
    let svg_w = inner_w + PAD * 2;
    let svg_h = NODE_H + PAD * 2 + 32; // extra for labels
    let mut positions: std::collections::HashMap<&str, (i32, i32)> =
        std::collections::HashMap::new();

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
        r##"<figure class="smilie-diagram"><svg viewBox="0 0 {svg_w} {svg_h}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="flow diagram">
<defs>
  <marker id="smilie-arrow" viewBox="0 0 12 12" refX="10" refY="6" markerWidth="10" markerHeight="10" orient="auto">
    <path d="M0 0 L12 6 L0 12 z" fill="#9a6bff"/>
  </marker>
</defs>"##
    );

    // Nodes
    for (i, node) in nodes.iter().enumerate() {
        let x = PAD + (i as i32) * (NODE_W + GAP_X);
        let y = PAD;
        positions.insert(node.id.as_str(), (x, y));

        let _ = write!(
            out,
            r##"<g class="smilie-diagram-node">
<rect x="{x}" y="{y}" width="{NODE_W}" height="{NODE_H}" rx="12" ry="12"
  fill="#15101f" stroke="#9a6bff" stroke-width="{STROKE_W}"/>
<text x="{tx}" y="{ty}" fill="#f3ecff" font-family="-apple-system,Inter,sans-serif"
  font-size="14" font-weight="600" text-anchor="middle">{label}</text>"##,
            tx = x + NODE_W / 2,
            ty = y + 28,
            label = escape(&node.label),
        );
        if let Some(note) = &node.note {
            if !note.trim().is_empty() {
                let _ = write!(
                    out,
                    r##"<text x="{tx}" y="{ty}" fill="#a498c4" font-family="-apple-system,Inter,sans-serif" font-size="11" text-anchor="middle">{note}</text>"##,
                    tx = x + NODE_W / 2,
                    ty = y + 48,
                    note = escape(note),
                );
            }
        }
        let _ = write!(out, "</g>");
    }

    // Edges
    for edge in edges {
        let from = positions.get(edge.from.as_str());
        let to = positions.get(edge.to.as_str());
        if let (Some((fx, fy)), Some((tx, ty))) = (from, to) {
            let x1 = fx + NODE_W;
            let y1 = fy + NODE_H / 2;
            let x2 = *tx;
            let y2 = ty + NODE_H / 2;
            let _ = write!(
                out,
                r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#9a6bff" stroke-width="2" marker-end="url(#smilie-arrow)" stroke-linecap="round"/>"##
            );
            if let Some(label) = &edge.label {
                if !label.trim().is_empty() {
                    let mx = (x1 + x2) / 2;
                    let my = y1 - 8;
                    let _ = write!(
                        out,
                        r##"<text x="{mx}" y="{my}" fill="#c8a8ff" font-family="-apple-system,Inter,sans-serif" font-size="11" text-anchor="middle">{label}</text>"##,
                        label = escape(label),
                    );
                }
            }
        }
    }

    out.push_str("</svg></figure>");
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
        // Stroke must be set (no black voids)
        assert!(html.contains(r##"stroke="#9a6bff""##));
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
    fn embedded_sdk_contains_offline_math_renderer() {
        let html = embed_page_sdk("<html><head></head><body></body></html>");
        assert!(html.contains("var temml="));
        assert!(html.contains("function renderMath"));
        assert!(!html.contains("https://"));
    }
}
