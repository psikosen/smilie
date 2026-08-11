# Security notes

## Main-window Content Security Policy

Source of truth: `src-tauri/tauri.conf.json` → `app.security.csp`.

Current CSP (production / packaged builds):

```
default-src 'self';
img-src 'self' asset: http://asset.localhost data: file: blob:;
style-src-elem 'self';
style-src-attr 'unsafe-inline';
script-src 'self';
connect-src 'self' ipc: http://ipc.localhost;
font-src 'self' data:;
frame-src 'self' asset: http://asset.localhost file: blob:
```

### Why `style-src` is split

Previously the policy used `style-src 'self' 'unsafe-inline'`, which allowed both
inline `<style>` blocks and inline `style=""` attributes. The single biggest
XSS-amplification vector in that policy was the `<style>` block: an injected
`<style>` can exfiltrate values via attribute selectors + `background-image: url(...)`,
overlay UI for clickjacking, or hide DOM. Inline `style=""` attributes are far
narrower (no selectors, no pseudo-classes, no `@import`).

The build confirmed via `npm run build` emits **only external CSS** (every Svelte
component's scoped styles compile into `_app/immutable/assets/*.css`), so we can
forbid inline `<style>` outright without breaking anything:

- `style-src-elem 'self'` blocks all inline `<style>` blocks (dev-mode HMR is
  not affected because Tauri's CSP is only enforced for the `tauri://` custom
  protocol used by packaged / production builds — Vite's dev server has no CSP).
- `style-src-attr 'unsafe-inline'` keeps:
  - `<div id="app-root" style="display: contents">` in `src/app.html`
  - The dynamic `style="background: {statusColor}"` in `PaperCard.svelte`
  - All `el.style.setProperty(...)` calls (cursor-glow, layout root vars in
    `src/lib/theme/effects.ts` and `src/routes/+layout.svelte`)

### Why we did not use a nonce or hash

CSP nonces and hashes apply to `<style>` and `<script>` *elements* only — they
do not authorize inline `style=""` attributes, regardless of source. Since the
attributes are required at runtime (theme cursor glow, dynamic dot color), a
nonce would not have removed the need for `'unsafe-inline'` somewhere; the
split-directive approach is strictly tighter than nonce-only because inline
`<style>` would still require `'unsafe-inline'` or per-block nonces and we have
neither. Hashes are equally inapplicable to attribute styles and would be
brittle (every Svelte rebuild would change the hash).

### Iframe (learning-page) CSP

The iframe-rendered learning content has its own, stricter CSP set inline in
the rendered HTML (see `src-tauri/src/pipeline/render`). That CSP is independent
of the main-window CSP and is intentionally not affected by changes here.

## Verification performed

- `npm run build` — succeeded, no inline `<style>` blocks in `build/`.
- `npm run check` — 0 errors, 0 warnings.
- `npx tsc -b --noEmit` — clean.
- `grep` over `build/` confirms only external `.css` files; only `style="..."`
  attributes remain inline (covered by `style-src-attr 'unsafe-inline'`).
- Source audit found two literal `style=""` usages in `src/` (`app.html`,
  `PaperCard.svelte`) and three `el.style.setProperty(...)` call sites
  (`effects.ts`, `+layout.svelte`); all remain functional under the new CSP.

Not run as part of this change:

- `npm run tauri:dev` live verification — startup is slow and the dev
  server is not subject to the CSP being changed (Tauri's CSP only applies
  to packaged builds), so devtools-console verification needs a packaged
  build run. The next person to run `npm run tauri:build` should open
  devtools and confirm no `Content Security Policy` violation lines on the
  library, paper detail, and settings pages, particularly while moving the
  cursor (exercises `setProperty` cursor glow) and while a paper card
  re-renders with status colors.
