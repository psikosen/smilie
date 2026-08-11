/**
 * Smilie Page SDK — injected into every generated learning page.
 *
 * Gives the generator model a small, stable set of primitives so pages
 * stay consistent + lightweight + offline. No external dependencies.
 *
 * Usage inside a generated page:
 *   <script src="/page-sdk.js"></script>
 *   <script>
 *     Smilie.applyTheme();
 *     Smilie.ready();
 *   </script>
 */
(function (global) {
  "use strict";

  // ── Theme injection ─────────────────────────────────────────
  // CSS is structured in three layers:
  //   1. Tokens — palette + spacing primitives.
  //   2. Guardrails — defensive rules so a free-form / legacy page can
  //      never overflow horizontally, render text in an invisible color,
  //      or leave SVG primitives unstyled (the "black void" failure
  //      mode we saw in early Qwen output).
  //   3. Block styling — visuals for every block type the Rust renderer
  //      emits (smilie-page-*, smilie-h*, smilie-callout-*, etc.).
  const THEME_CSS = `
    :root {
      --bg-void:    #090a0d;
      --bg-panel:   #111318;
      --bg-elev:    #171a20;
      --violet-50:  #f2edff;
      --violet-100: #d1c3ff;
      --violet-200: #b39aff;
      --violet-300: #9474ef;
      --violet-400: #7655d7;
      --violet-500: #6241c4;
      --violet-glow:#a78cff;
      --cyan:       #65d4dd;
      --magenta:    #e976aa;
      --text-hi:    #f3f4f7;
      --text-mid:   #b1b6c1;
      --text-dim:   #747b88;
      --border:     rgba(181,187,201,0.16);
      --border-strong: rgba(148,116,239,0.48);
      --status-ok:  #72d6a0;
      --status-warn:#efbd61;
      --status-err: #ef7d8b;
      --radius-sm:  6px;
      --radius-md:  8px;
      --radius-lg:  8px;
      --font-sans:  "Geist", "Inter", system-ui, -apple-system, sans-serif;
      --font-mono:  "Geist Mono", "JetBrains Mono", ui-monospace, monospace;
    }

    /* Guardrails — keep ANY emitted document on the rails. */
    *, *::before, *::after { box-sizing: border-box; }
    html, body {
      margin: 0; padding: 0;
      background: var(--bg-void);
      color: var(--text-hi);
      font-family: var(--font-sans);
      max-width: 100%;
      overflow-x: hidden;
    }
    body {
      padding: clamp(24px, 4vw, 52px) clamp(18px, 5vw, 64px) 64px;
      line-height: 1.68;
      font-size: 16px;
      word-break: normal;
      overflow-wrap: anywhere;
    }
    img, svg, video, canvas, iframe, table {
      max-width: 100%;
      height: auto;
    }
    /* Default styling for raw SVG primitives — prevents the "black box"
       failure where a model emits <rect> with no fill. */
    svg rect:not([fill]):not([style*="fill"]),
    svg circle:not([fill]):not([style*="fill"]),
    svg ellipse:not([fill]):not([style*="fill"]),
    svg polygon:not([fill]):not([style*="fill"]) {
      fill: rgba(154,107,255,0.12);
      stroke: var(--violet-300);
      stroke-width: 1.5;
    }
    svg line:not([stroke]):not([style*="stroke"]),
    svg path:not([stroke]):not([style*="stroke"]) {
      stroke: var(--violet-300);
      stroke-width: 1.5;
      fill: none;
    }
    svg text:not([fill]):not([style*="fill"]) { fill: var(--text-hi); }

    /* Generic typography */
    h1, h2, h3, h4 { letter-spacing: 0; color: var(--text-hi); margin: 1.5em 0 0.55em; line-height: 1.25; }
    h1 { font-size: 32px; }
    h2 { font-size: 23px; }
    h3 { font-size: 18px; }
    p  { color: var(--text-hi); margin: 0 0 1em; }
    a  { color: var(--violet-100); }
    code, pre, .mono { font-family: var(--font-mono); }
    pre { padding: 14px 16px; border-radius: var(--radius-md);
          background: var(--bg-elev); border: 1px solid var(--border);
          overflow-x: auto; }
    ul, ol { padding-left: 1.4em; margin: 0 0 1em; }
    li { margin: 0.25em 0; }

    /* Block styling — produced by pipeline::render */
    .smilie-page-head, .smilie-page-body { width: min(100%, 1080px); margin-inline: auto; }
    .smilie-page-head { margin-bottom: 28px; padding-bottom: 22px;
      border-bottom: 1px solid var(--border); }
    .smilie-page-title {
      margin: 0; max-width: 820px; font-weight: 650; letter-spacing: 0;
      color: var(--text-hi);
    }
    .smilie-page-body { display: flex; flex-direction: column; gap: 6px; }
    .smilie-h2, .smilie-h3, .smilie-h4 { color: var(--text-hi); }
    .smilie-p { color: var(--text-hi); margin: 0 0 0.9em; }

    .smilie-callout {
      display: flex; flex-direction: column; gap: 6px;
      padding: 16px 18px; margin: 14px 0;
      border-radius: var(--radius-md);
      border: 1px solid var(--border); border-left: 3px solid var(--violet-300);
      background: rgba(148,116,239,0.055);
    }
    .smilie-callout-title {
      font-weight: 600; font-size: 13px; letter-spacing: 0.04em;
      text-transform: uppercase; color: var(--violet-100);
    }
    .smilie-callout-body { color: var(--text-hi); }
    .smilie-callout-warn { background: rgba(239,189,97,0.055); border-color: rgba(239,189,97,0.3); border-left-color: var(--status-warn); }
    .smilie-callout-warn .smilie-callout-title { color: var(--status-warn); }
    .smilie-callout-key  { background: rgba(101,212,221,0.055); border-color: rgba(101,212,221,0.3); border-left-color: var(--cyan); }
    .smilie-callout-key  .smilie-callout-title { color: var(--cyan); }
    .smilie-callout-note { background: rgba(255,255,255,0.02); border-color: var(--border); }

    .smilie-card {
      background: var(--bg-panel);
      border: 1px solid var(--border); border-radius: var(--radius-lg);
      padding: 18px; margin: 14px 0;
    }
    .smilie-keyterm .smilie-term {
      font-weight: 600; color: var(--violet-100);
      letter-spacing: 0.02em; margin-bottom: 4px;
    }
    .smilie-keyterm .smilie-definition { color: var(--text-hi); }

    .smilie-formula {
      margin: 14px 0; padding: 16px 18px;
      border-radius: var(--radius-md);
      background: var(--bg-elev); border: 1px solid var(--border);
    }
    .smilie-formula-render {
      display: block; min-height: 2.4em; padding: 8px 4px;
      overflow-x: auto; overflow-y: hidden; text-align: center;
      color: var(--text-hi); font-size: 1.15em;
    }
    .smilie-formula-render math[display="block"] { margin: 0 auto; }
    .smilie-formula-fallback {
      color: var(--violet-100); white-space: pre-wrap; overflow-wrap: anywhere;
    }
    .smilie-formula-render.has-error { text-align: left; }
    .smilie-formula-caption { font-size: 13px; color: var(--text-mid); margin-top: 8px; text-align: center; }

    .smilie-list { color: var(--text-hi); }

    .smilie-source-ref {
      display: flex; flex-wrap: wrap; gap: 6px;
      margin: 18px 0 0;
      padding-top: 14px;
      border-top: 1px dashed var(--border);
    }
    .smilie-chip {
      display: inline-block; padding: 2px 10px; border-radius: 999px;
      background: rgba(154,107,255,0.1); color: var(--violet-100);
      font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase;
    }

    .smilie-diagram {
      margin: 10px 0 16px; padding: 18px;
      background: var(--bg-panel);
      border: 1px solid var(--border); border-radius: var(--radius-lg);
    }
    .smilie-diagram-title {
      font-size: 12px; letter-spacing: 0.08em; text-transform: uppercase;
      color: var(--violet-200); margin: 20px 0 8px;
    }
    .smilie-flow {
      display: grid;
      grid-template-columns: repeat(var(--smilie-flow-count), minmax(0, 1fr));
      gap: 0;
      margin: 0;
      padding: 0;
      list-style: none;
    }
    .smilie-flow-step {
      display: grid;
      grid-template-columns: minmax(0, 1fr) 72px;
      align-items: center;
      min-width: 0;
      margin: 0;
    }
    .smilie-flow-node {
      position: relative;
      display: flex;
      min-width: 0;
      min-height: 128px;
      flex-direction: column;
      justify-content: center;
      gap: 8px;
      padding: 24px 16px 18px;
      border: 1px solid var(--border-strong);
      border-radius: var(--radius-md);
      background: var(--bg-elev);
    }
    .smilie-flow-node strong { font-size: 14px; line-height: 1.35; color: var(--text-hi); }
    .smilie-flow-index {
      position: absolute;
      top: 8px;
      left: 10px;
      color: var(--cyan);
      font-family: var(--font-mono);
      font-size: 10px;
    }
    .smilie-flow-note { color: var(--text-mid); font-size: 12px; line-height: 1.45; }
    .smilie-flow-link {
      display: flex;
      min-width: 42px;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 4px;
      padding: 6px;
      color: var(--violet-100);
      text-align: center;
    }
    .smilie-flow-link span { width: 100%; font-size: 10px; line-height: 1.25; }
    .smilie-flow-link b { color: var(--violet-300); font-size: 20px; font-weight: 500; }
    .smilie-flow-relations {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-top: 14px;
      padding-top: 14px;
      border-top: 1px solid var(--border);
    }
    .smilie-flow-relations > span {
      padding: 6px 8px;
      border-radius: var(--radius-sm);
      background: rgba(101,212,221,0.06);
      color: var(--text-mid);
      font-size: 11px;
    }

    .smilie-quiz { display: flex; flex-direction: column; gap: 10px; margin: 14px 0; }
    .smilie-quiz-error { color: var(--status-err); }

    .smilie-code { font-family: var(--font-mono); font-size: 13px; color: var(--text-hi); }

    button.smilie-btn {
      padding: 8px 14px; border-radius: 999px; border: 1px solid var(--border);
      background: var(--bg-elev); color: var(--text-hi);
      cursor: pointer; font: inherit;
    }
    button.smilie-btn:hover { border-color: var(--violet-300); box-shadow: 0 0 14px -4px var(--violet-glow); }
    input[type="range"].smilie-slider { accent-color: var(--violet-500); width: 100%; }

    @media (max-width: 900px) {
      body { padding-inline: 18px; }
      .smilie-flow { grid-template-columns: minmax(0, 1fr); gap: 0; }
      .smilie-flow-step,
      .smilie-flow-step:last-child { grid-template-columns: minmax(0, 1fr); }
      .smilie-flow-node { min-height: 0; }
      .smilie-flow-link { min-height: 64px; padding: 8px 12px; }
      .smilie-flow-link span { max-width: min(100%, 360px); }
      .smilie-flow-link b { transform: rotate(90deg); }
    }
  `;

  function applyTheme() {
    if (document.getElementById("smilie-theme")) return;
    const style = document.createElement("style");
    style.id = "smilie-theme";
    style.textContent = THEME_CSS;
    document.head.appendChild(style);
  }

  // ── Offline LaTeX → MathML ───────────────────────────────
  // Temml is injected immediately before this SDK. Native MathML keeps
  // formulas crisp, accessible, and independent of network/font assets.
  function renderMath(root) {
    const scope = root && root.querySelectorAll ? root : document;
    scope.querySelectorAll(".smilie-formula-render[data-latex]").forEach((el) => {
      const latex = el.getAttribute("data-latex") || "";
      if (!latex.trim()) return;
      const fallback = el.innerHTML;
      try {
        if (!global.temml || typeof global.temml.render !== "function") {
          throw new Error("Temml is unavailable");
        }
        global.temml.render(latex, el, { displayMode: true, throwOnError: true });
        el.classList.add("is-rendered");
        el.removeAttribute("data-latex");
      } catch (_error) {
        el.innerHTML = fallback;
        el.classList.add("has-error");
        el.title = "This formula could not be rendered";
      }
    });
  }

  document.addEventListener("smilie:ready", () => renderMath(document));
  document.addEventListener("DOMContentLoaded", () => renderMath(document));

  // ── Tiny plot primitive (no dependencies) ──────────────────
  function plot(canvas, opts) {
    const ctx = canvas.getContext("2d");
    const xs = opts.x || [];
    const ys = opts.y || [];
    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);
    if (!xs.length || !ys.length) return;

    const minX = Math.min(...xs), maxX = Math.max(...xs);
    const minY = Math.min(...ys), maxY = Math.max(...ys);
    const padX = 28, padY = 22;

    // axes
    ctx.strokeStyle = "rgba(164,152,196,0.4)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(padX, padY);
    ctx.lineTo(padX, h - padY);
    ctx.lineTo(w - padX, h - padY);
    ctx.stroke();

    // line
    ctx.strokeStyle = opts.color || "#b580ff";
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let i = 0; i < xs.length; i++) {
      const px = padX + ((xs[i] - minX) / (maxX - minX || 1)) * (w - padX * 2);
      const py = h - padY - ((ys[i] - minY) / (maxY - minY || 1)) * (h - padY * 2);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.stroke();
  }

  // ── Slider primitive ───────────────────────────────────────
  function slider(container, opts) {
    const input = document.createElement("input");
    input.type = "range";
    input.className = "smilie-slider";
    input.min = String(opts.min ?? 0);
    input.max = String(opts.max ?? 100);
    input.step = String(opts.step ?? 1);
    input.value = String(opts.value ?? opts.min ?? 0);
    const label = document.createElement("div");
    label.className = "mono";
    label.style.fontSize = "12px";
    label.style.color = "var(--text-mid)";
    function render() {
      label.textContent = `${opts.label || "value"}: ${input.value}`;
    }
    render();
    input.addEventListener("input", () => {
      render();
      opts.onChange && opts.onChange(Number(input.value));
    });
    container.appendChild(label);
    container.appendChild(input);
    return input;
  }

  // ── Quiz primitive ─────────────────────────────────────────
  // When the user has answered every question we POST the attempt up to
  // the host (Tauri) so it can record a row in `quiz_attempts` and update
  // the SRS schedule. We only fire ONCE per quiz instance, even if the
  // SDK is re-evaluated (rare; iframes are torn down and rebuilt on nav).
  function recordAttempt(correct, total) {
    try {
      const meta = document.querySelector('meta[name="smilie-page-id"]');
      const id = meta && meta.content;
      if (!id) return;
      // `parent` is the host SvelteKit window which has Tauri access.
      // `*` is fine — the only data we send is correct/total, no secrets.
      parent.postMessage(
        { type: "smilie:quiz_attempt", page_id: id, correct: correct, total: total },
        "*"
      );
    } catch (_e) {
      // We never want to break a page render because of an SRS hiccup.
    }
  }

  function quiz(container, questions) {
    const state = {
      correct: 0,
      total: questions.length,
      answered: new Set(),
      reported: false,
    };
    const score = document.createElement("div");
    score.className = "smilie-chip";
    score.style.marginBottom = "10px";
    score.textContent = `0 / ${state.total}`;
    container.appendChild(score);

    questions.forEach((q, i) => {
      const card = document.createElement("div");
      card.className = "smilie-card";
      card.innerHTML = `<div style="font-weight:600;margin-bottom:10px">${escapeHtml(q.q)}</div>`;
      q.choices.forEach((c, ci) => {
        const btn = document.createElement("button");
        btn.className = "smilie-btn";
        btn.style.marginRight = "8px";
        btn.style.marginBottom = "6px";
        btn.textContent = c;
        btn.addEventListener("click", () => {
          if (state.answered.has(i)) return;
          state.answered.add(i);
          const correct = ci === q.answer;
          btn.style.borderColor = correct ? "rgba(93,255,160,0.6)" : "rgba(255,107,123,0.6)";
          btn.style.color = correct ? "#5dffa0" : "#ff6b7b";
          if (correct) state.correct += 1;
          score.textContent = `${state.correct} / ${state.total}`;
          if (
            !state.reported &&
            state.total > 0 &&
            state.answered.size === state.total
          ) {
            state.reported = true;
            recordAttempt(state.correct, state.total);
          }
        });
        card.appendChild(btn);
      });
      container.appendChild(card);
    });
  }

  function escapeHtml(s) {
    const div = document.createElement("div");
    div.textContent = s;
    return div.innerHTML;
  }

  // ── Source reference ───────────────────────────────────────
  function source(sectionId) {
    const el = document.createElement("a");
    el.className = "smilie-chip";
    el.textContent = `source: ${sectionId}`;
    el.href = "#";
    el.addEventListener("click", (e) => {
      e.preventDefault();
      parent.postMessage({ type: "smilie:source", sectionId }, "*");
    });
    return el;
  }

  function ready() {
    renderMath(document);
    document.dispatchEvent(new CustomEvent("smilie:ready"));
  }

  global.Smilie = { applyTheme, renderMath, plot, slider, quiz, source, ready };
})(typeof window !== "undefined" ? window : globalThis);
