<script lang="ts">
  import { tick } from "svelte";
  import type { PageRecord, PaperSummary } from "$lib/ipc";
  import { viewport } from "$lib/stores/viewport";

  interface Props {
    paper: PaperSummary;
    pages: PageRecord[];
    activePageId: string | null;
    /** Desktop: rail width toggle (collapsed = 48px icon rail). */
    collapsed: boolean;
    /** Mobile: open = bottom sheet visible. Ignored on desktop. */
    mobileOpen: boolean;
    onSelect: (id: string) => void;
    onToggle: () => void;
    onMobileClose: () => void;
  }
  let {
    paper,
    pages,
    activePageId,
    collapsed,
    mobileOpen,
    onSelect,
    onToggle,
    onMobileClose,
  }: Props = $props();
  let sidebarEl: HTMLElement | null = null;
  let wasMobileOpen = false;

  function iconForType(type: string): string {
    switch (type) {
      case "informational": return "I";
      case "concept":    return "C";
      case "formula":    return "Σ";
      case "figure":     return "F";
      case "table":      return "T";
      case "quiz":       return "?";
      case "simulation": return "▶";
      default:           return "P";
    }
  }

  const readyCount = $derived(pages.filter((p) => p.status === "ready").length);
  const generatingCount = $derived(
    pages.filter((p) => p.status === "queued" || p.status === "generating").length,
  );
  const failedCount = $derived(pages.filter((p) => p.status === "failed").length);
  const savedSummary = $derived.by(() => {
    const parts = [`${readyCount} saved offline`];
    if (generatingCount) parts.push(`${generatingCount} in progress`);
    if (failedCount) parts.push(`${failedCount} failed`);
    return parts.join(" · ");
  });

  function typeLabel(type: string): string {
    switch (type) {
      case "informational": return "Informational page";
      case "concept": return "Concept explainer";
      case "formula": return "Formula explorer";
      case "figure": return "Figure explorer";
      case "table": return "Table explorer";
      case "quiz": return "Quiz";
      case "simulation": return "Simulation";
      default: return "Learning page";
    }
  }

  function statusLabel(status: PageRecord["status"]): string {
    switch (status) {
      case "ready": return "Saved offline";
      case "queued": return "Queued";
      case "generating": return "Generating";
      case "failed": return "Generation failed";
    }
  }

  $effect(() => {
    const open = $viewport.mobile && mobileOpen;
    if (open && !wasMobileOpen) {
      void tick().then(() => {
        const active = sidebarEl?.querySelector<HTMLElement>(
          '[aria-current="page"]',
        );
        (active ?? sidebarEl)?.focus();
      });
    }
    wasMobileOpen = open;
  });

  function pickPage(id: string) {
    onSelect(id);
    if ($viewport.mobile) onMobileClose();
  }

  function handleMobileDialogKeydown(event: KeyboardEvent) {
    if (!$viewport.mobile || !mobileOpen) return;
    if (event.key === "Escape") {
      event.preventDefault();
      onMobileClose();
      return;
    }
    if (event.key !== "Tab" || !sidebarEl) return;
    const focusable = Array.from(
      sidebarEl.querySelectorAll<HTMLElement>(
        'button:not(:disabled), [href], [tabindex]:not([tabindex="-1"])',
      ),
    );
    if (focusable.length === 0) {
      event.preventDefault();
      sidebarEl.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

{#if $viewport.mobile && mobileOpen}
  <button
    class="backdrop"
    type="button"
    aria-label="Close pages menu"
    onclick={onMobileClose}
    tabindex="-1"
  ></button>
{/if}

<aside
  id="paper-pages-sidebar"
  class="sidebar"
  class:collapsed={collapsed && !$viewport.mobile}
  class:mobile={$viewport.mobile}
  class:open={mobileOpen}
  aria-label="Learning pages"
  aria-hidden={$viewport.mobile && !mobileOpen}
  aria-modal={$viewport.mobile && mobileOpen ? "true" : undefined}
  role={$viewport.mobile ? "dialog" : undefined}
  inert={$viewport.mobile && !mobileOpen}
  tabindex="-1"
  bind:this={sidebarEl}
  onkeydown={handleMobileDialogKeydown}
>
  {#if $viewport.mobile}
    <button
      type="button"
      class="grip"
      onclick={onMobileClose}
      aria-label="Close pages"
    >
      <span class="grip-bar" aria-hidden="true"></span>
    </button>
  {/if}

  <div class="head">
    {#if !$viewport.mobile}
      <button
        class="toggle"
        type="button"
        onclick={onToggle}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none">
          <path
            d={collapsed ? "M6 3l5 5-5 5" : "M10 3L5 8l5 5"}
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    {/if}
    {#if !collapsed || $viewport.mobile}
      <div class="head-copy">
        <div class="paper-title" title={paper.title}>
          {paper.title || "Untitled"}
        </div>
        <div class="paper-meta">
          {savedSummary}
        </div>
      </div>
    {/if}
  </div>

  <nav class="list" aria-label="Paper pages">
    {#if pages.length === 0}
      {#if !collapsed || $viewport.mobile}
        <div class="empty" role="status">
          Learning pages will appear here as the pipeline saves them.
        </div>
      {/if}
    {:else}
      {#each pages as p (p.id)}
        <button
          class="item"
          class:active={activePageId === p.id}
          class:ghost={p.status !== "ready"}
          class:failed={p.status === "failed"}
          type="button"
          onclick={() => pickPage(p.id)}
          title={p.title}
          aria-current={activePageId === p.id ? "page" : undefined}
          aria-label={`${p.title}. ${typeLabel(p.type)}. ${statusLabel(p.status)}.`}
        >
          <span
            class="glyph"
            class:ready={p.status === "ready"}
            class:failed={p.status === "failed"}
            aria-hidden="true"
          >{iconForType(p.type)}</span>
          {#if !collapsed || $viewport.mobile}
            <span class="label">{p.title}</span>
            <span
              class="status-chip"
              class:ready={p.status === "ready"}
              class:failed={p.status === "failed"}
            >
              {p.status === "ready" ? "saved" : p.status}
            </span>
          {/if}
        </button>
      {/each}
    {/if}
  </nav>
</aside>

<style>
  .sidebar {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 280px;
    padding: 14px 10px;
    background:
      radial-gradient(420px 300px at 0% 0%, rgba(154, 107, 255, 0.13), transparent 64%),
      linear-gradient(180deg, rgba(13, 10, 24, 0.96), var(--bg-panel));
    border-right: 1px solid var(--border);
    overflow-y: auto;
    transition: width var(--dur) var(--ease);
    box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.025);
    outline: none;
  }

  .sidebar.collapsed {
    width: 48px;
    padding: 14px 4px;
  }

  /* Mobile: bottom sheet. Slides up when `open`. */
  .sidebar.mobile {
    position: fixed;
    inset: auto 0 0 0;
    width: 100%;
    height: min(75dvh, 600px);
    max-height: 75dvh;
    padding: 6px 12px 24px;
    border-right: 0;
    border-top: 1px solid var(--border-strong);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    box-shadow: 0 -28px 60px -20px rgba(0, 0, 0, 0.7);
    transform: translateY(100%);
    transition: transform var(--dur-slow) var(--ease);
    z-index: var(--z-drawer);
    /* Avoid the iOS home indicator + notch. */
    padding-bottom: max(24px, env(safe-area-inset-bottom));
  }

  .sidebar.mobile.open { transform: translateY(0); }

  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--bg-overlay);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    border: 0;
    cursor: pointer;
    z-index: calc(var(--z-drawer) - 1);
    animation: fade-in var(--dur) var(--ease);
  }

  @keyframes fade-in { from { opacity: 0 } to { opacity: 1 } }

  .grip {
    align-self: center;
    width: 48px;
    height: 28px;
    padding: 8px 0;
    margin: -2px 0 4px;
    background: transparent;
    border: 0;
    cursor: grab;
    display: grid;
    place-items: center;
  }

  .grip-bar {
    width: 36px;
    height: 4px;
    border-radius: 2px;
    background: rgba(154, 107, 255, 0.4);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    padding: 0 6px 12px;
    border-bottom: 1px solid var(--divider);
  }

  .sidebar.mobile .head { padding: 4px 8px 8px; }

  .toggle {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    color: var(--text-mid);
    border: 1px solid var(--border);
    background:
      linear-gradient(145deg, rgba(255, 255, 255, 0.04), transparent),
      var(--bg-elevated);
    flex: none;
    transition: color var(--dur) var(--ease), background var(--dur) var(--ease);
  }
  .toggle:hover { color: var(--violet-100); background: rgba(154,107,255,0.1); }

  .head-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .paper-title {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-hi);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .sidebar.mobile .paper-title { font-size: var(--text-md); }

  .paper-meta {
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .empty {
    margin: 18px 6px;
    font-size: var(--text-xs);
    color: var(--text-dim);
    line-height: 1.5;
  }

  .item {
    display: grid;
    grid-template-columns: 24px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text-mid);
    font-size: var(--text-sm);
    transition:
      color var(--dur) var(--ease),
      background var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
    min-height: 40px;
    border: 1px solid transparent;
  }

  /* Bigger touch targets on mobile. */
  .sidebar.mobile .item {
    min-height: 48px;
    padding: 12px 12px;
    font-size: var(--text-md);
    border-radius: var(--radius-md);
  }

  .sidebar.collapsed .item {
    grid-template-columns: 1fr;
    justify-items: center;
  }

  .item:hover {
    color: var(--text-hi);
    background: rgba(154,107,255,0.065);
    border-color: var(--border-subtle);
  }

  .item.active {
    color: var(--text-hi);
    background: linear-gradient(
      90deg,
      rgba(154, 107, 255, 0.22),
      rgba(79, 240, 255, 0.06) 70%
    );
    border-color: var(--border);
    box-shadow:
      inset 2px 0 0 var(--violet-400),
      0 0 18px -12px var(--violet-glow);
  }

  .item.ghost { color: var(--text-dim); }

  .item.ghost.failed {
    color: var(--status-err);
  }

  .glyph {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--violet-200);
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    background: rgba(255, 255, 255, 0.025);
    text-align: center;
  }
  .sidebar.mobile .glyph { font-size: 16px; }

  .glyph.ready {
    border-color: rgba(93, 255, 160, 0.2);
    box-shadow: inset 0 0 8px rgba(93, 255, 160, 0.04);
  }

  .glyph.failed {
    color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.3);
  }

  .label {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status-chip {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: var(--radius-pill);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-dim);
    border: 1px solid var(--border-subtle);
  }

  .status-chip.ready {
    color: var(--status-ok);
    border-color: rgba(93, 255, 160, 0.22);
    background: rgba(93, 255, 160, 0.055);
  }

  .status-chip.failed {
    color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.28);
    background: rgba(255, 107, 123, 0.07);
  }

  .toggle:focus-visible,
  .item:focus-visible,
  .grip:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }
</style>
