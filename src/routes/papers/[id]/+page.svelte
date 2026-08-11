<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import TopBar from "$lib/components/TopBar.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import PageViewer from "$lib/components/PageViewer.svelte";
  import ProgressPanel from "$lib/components/ProgressPanel.svelte";
  import MissingWeightsBanner from "$lib/components/MissingWeightsBanner.svelte";
  import ExperimentTab from "$lib/components/ExperimentTab.svelte";
  import PdfTab from "$lib/components/PdfTab.svelte";
  import CompareTab from "$lib/components/CompareTab.svelte";
  import { paperDetail } from "$lib/stores/paper_detail";
  import { jobs } from "$lib/stores/jobs";
  import { viewport } from "$lib/stores/viewport";
  import { activePage as activePageStore } from "$lib/stores/active_page";
  import { closeRequest } from "$lib/stores/drawer";
  import { ipc, type PageRecord } from "$lib/ipc";
  import { toast } from "$lib/stores/toast";

  const paperId = $derived(page.params.id);

  let activePageId = $state<string | null>(null);
  let pendingPageUrl = $state<string | null>(null);
  let sidebarCollapsed = $state(false);
  let mobileSheetOpen = $state(false);
  let openedPaperId = $state<string | null>(null);
  let pagesFab = $state<HTMLButtonElement | null>(null);
  // Tab strip — local-route state.
  //   pages       — existing learning-pages view (sidebar visible)
  //   experiments — Pyodide-runner reproductions (sidebar hidden)
  //   pdf         — original PDF in an iframe (sidebar hidden)
  //   compare     — two-paper diff picker + viewer (sidebar hidden)
  type Tab = "pages" | "experiments" | "pdf" | "compare";
  const TABS: Tab[] = ["pages", "experiments", "pdf", "compare"];
  const requestedTab = $derived.by<Tab>(() => {
    const candidate = page.url.searchParams.get("view");
    return TABS.includes(candidate as Tab) ? (candidate as Tab) : "pages";
  });
  let activeTab = $state<Tab>("pages");

  // Sidebar is only useful in pages-mode — the other tabs own their full
  // width. Mobile keeps its existing FAB-driven behavior regardless of
  // the active tab so the user can still reach the page list.
  const sidebarVisible = $derived(activeTab === "pages");

  $effect(() => {
    if (paperId) {
      if (openedPaperId !== paperId) {
        openedPaperId = paperId;
        activePageId = null;
        pendingPageUrl = null;
        activeTab = requestedTab;
        mobileSheetOpen = false;
      }
      void paperDetail.open(paperId);
      void jobs.init();
    }
  });

  onDestroy(() => {
    paperDetail.close();
    activePageStore.clear();
  });

  // Auto-select first ready page once pages arrive. Honors a `?page=<id>`
  // query param (used by deep-links from the library "Due for review"
  // row) when the requested page actually exists in this paper.
  $effect(() => {
    const pages = $paperDetail.paper?.pages ?? [];
    const urlRequested = page.url.searchParams.get("page");
    if (pendingPageUrl && urlRequested === pendingPageUrl) pendingPageUrl = null;
    const requested = pendingPageUrl ?? urlRequested;
    const matched = requested ? pages.find((p) => p.id === requested) : null;
    const activeStillExists = pages.some((p) => p.id === activePageId);
    if (pages.length === 0) {
      activePageId = null;
    } else if (matched && matched.id !== activePageId) {
      activePageId = matched.id;
    } else if (!activeStillExists) {
      const firstReady = pages.find((p) => p.status === "ready") ?? pages[0];
      activePageId = (matched ?? firstReady)?.id ?? null;
    }
    // Publish to the layout-level shortcut handler so J/K + arrows can
    // step prev/next without coupling the keyhandler to this route.
    activePageStore.set({
      pages,
      activePageId,
      setActivePageId: selectPage,
    });
  });

  // Close the mobile sheet when the viewport flips back to desktop so
  // the sidebar reverts to inline rendering cleanly.
  $effect(() => {
    if (!$viewport.mobile && mobileSheetOpen) {
      mobileSheetOpen = false;
    }
  });

  // Esc / global close request -> dismiss the mobile sheet.
  $effect(() => {
    $closeRequest;
    if (mobileSheetOpen) void closeMobileSheet();
  });

  const activePage = $derived<PageRecord | null>(
    $paperDetail.paper?.pages.find((p) => p.id === activePageId) ?? null,
  );

  const pageCount = $derived($paperDetail.paper?.pages.length ?? 0);
  const readyCount = $derived(
    $paperDetail.paper?.pages.filter((p) => p.status === "ready").length ?? 0,
  );

  // Detect a "missing weights" failure from the active job's error_message.
  // Backend formats `RuntimeError::MissingWeights { path }` as
  // `model weights not found at <path>` via `format!("{e:#}")`. Match that
  // exact phrasing — anything else we leave to the existing error UI.
  const MISSING_WEIGHTS_RE = /model weights not found at (.+?)(?:\n|$)/m;
  const missingWeights = $derived.by<{ path: string; message: string } | null>(
    () => {
      const job = $paperDetail.paper?.active_job;
      if (!job || job.status !== "failed") return null;
      const msg = job.error_message;
      if (!msg) return null;
      const match = msg.match(MISSING_WEIGHTS_RE);
      if (!match) return null;
      return { path: match[1].trim(), message: msg };
    },
  );

  async function regenerate(id: string) {
    try {
      await ipc.regeneratePage(id);
      toast.success("Page queued for regeneration");
    } catch (e) {
      console.error("regenerate failed", e);
      toast.error(`Could not regenerate page: ${String(e)}`);
    }
  }

  function replaceWorkspaceUrl(nextPageId = activePageId, nextTab = activeTab) {
    const url = new URL(page.url);
    if (nextPageId) url.searchParams.set("page", nextPageId);
    else url.searchParams.delete("page");
    if (nextTab === "pages") url.searchParams.delete("view");
    else url.searchParams.set("view", nextTab);
    void goto(`${url.pathname}${url.search}${url.hash}`, {
      replaceState: true,
      noScroll: true,
      keepFocus: true,
    });
  }

  function selectPage(id: string) {
    pendingPageUrl = id;
    activePageId = id;
    replaceWorkspaceUrl(id, activeTab);
  }

  function selectTab(tab: Tab, focus = false) {
    activeTab = tab;
    mobileSheetOpen = false;
    replaceWorkspaceUrl(activePageId, tab);
    if (focus) {
      void tick().then(() => document.getElementById(`paper-tab-${tab}`)?.focus());
    }
  }

  function handleTabKeydown(event: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const current = TABS.indexOf(activeTab);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? TABS.length - 1
          : (current + (event.key === "ArrowRight" ? 1 : -1) + TABS.length) %
            TABS.length;
    selectTab(TABS[next], true);
  }

  async function closeMobileSheet() {
    mobileSheetOpen = false;
    await tick();
    pagesFab?.focus();
  }

  async function deletePaper() {
    if (!paperId) return;
    const paperTitle = $paperDetail.paper?.paper.title || "this paper";
    const ok = confirm(
      `Delete "${paperTitle}" and all ${readyCount} saved learning pages? This cannot be undone.`,
    );
    if (!ok) return;
    try {
      await ipc.deletePaper(paperId);
      void goto("/");
    } catch (error) {
      toast.error(`Could not delete paper: ${String(error)}`);
    }
  }
</script>

<TopBar onSettings={() => goto("/settings/")} onBack={() => goto("/")} />

<div
  class="layout"
  class:mobile={$viewport.mobile}
  class:no-sidebar={!sidebarVisible && !$viewport.mobile}
>
  {#if sidebarVisible || $viewport.mobile}
    {#if $paperDetail.paper}
      <Sidebar
        paper={$paperDetail.paper.paper}
        pages={$paperDetail.paper.pages}
        {activePageId}
        collapsed={sidebarCollapsed}
        mobileOpen={mobileSheetOpen}
        onSelect={selectPage}
        onToggle={() => (sidebarCollapsed = !sidebarCollapsed)}
        onMobileClose={() => void closeMobileSheet()}
      />
    {:else if !$viewport.mobile}
      <aside class="sidebar-loading" aria-busy="true"></aside>
    {/if}
  {/if}

  <main class="main" aria-label="Paper learning workspace">
    <div
      class="tab-strip"
      role="tablist"
      aria-label="Paper workspace views"
    >
      <button
        id="paper-tab-pages"
        type="button"
        role="tab"
        class="tab-btn"
        class:active={activeTab === "pages"}
        aria-selected={activeTab === "pages"}
        aria-controls="paper-panel-pages"
        tabindex={activeTab === "pages" ? 0 : -1}
        onclick={() => selectTab("pages")}
        onkeydown={handleTabKeydown}
      >
        Pages
      </button>
      <button
        id="paper-tab-experiments"
        type="button"
        role="tab"
        class="tab-btn"
        class:active={activeTab === "experiments"}
        aria-selected={activeTab === "experiments"}
        aria-controls="paper-panel-experiments"
        tabindex={activeTab === "experiments" ? 0 : -1}
        onclick={() => selectTab("experiments")}
        onkeydown={handleTabKeydown}
      >
        Experiments
      </button>
      <button
        id="paper-tab-pdf"
        type="button"
        role="tab"
        class="tab-btn"
        class:active={activeTab === "pdf"}
        aria-selected={activeTab === "pdf"}
        aria-controls="paper-panel-pdf"
        tabindex={activeTab === "pdf" ? 0 : -1}
        onclick={() => selectTab("pdf")}
        onkeydown={handleTabKeydown}
      >
        PDF
      </button>
      <button
        id="paper-tab-compare"
        type="button"
        role="tab"
        class="tab-btn"
        class:active={activeTab === "compare"}
        aria-selected={activeTab === "compare"}
        aria-controls="paper-panel-compare"
        tabindex={activeTab === "compare" ? 0 : -1}
        onclick={() => selectTab("compare")}
        onkeydown={handleTabKeydown}
      >
        Compare
      </button>
    </div>
    {#if missingWeights}
      <MissingWeightsBanner
        expectedPath={missingWeights.path}
        errorMessage={missingWeights.message}
        onSettings={() => goto("/settings/")}
      />
    {/if}
    <div
      class="tab-panel"
      class:has-mobile-fab={$viewport.mobile && activeTab === "pages"}
      id={`paper-panel-${activeTab}`}
      role="tabpanel"
      aria-labelledby={`paper-tab-${activeTab}`}
      tabindex="0"
    >
      {#if $paperDetail.error}
        <div class="fatal" role="alert">
          <h2>Could not load paper</h2>
          <p>{$paperDetail.error}</p>
          <button class="btn" type="button" onclick={() => goto("/")}>Back to library</button>
        </div>
      {:else if activeTab === "pages"}
        <PageViewer
          page={activePage}
          onRegenerate={regenerate}
          onDeletePaper={deletePaper}
        />
      {:else if activeTab === "experiments" && paperId}
        <ExperimentTab {paperId} />
      {:else if activeTab === "pdf" && paperId}
        <PdfTab {paperId} />
      {:else if activeTab === "compare" && paperId}
        <CompareTab {paperId} />
      {/if}
    </div>
  </main>

  {#if paperId}
    <ProgressPanel {paperId} />
  {/if}

  {#if $viewport.mobile && $paperDetail.paper && activeTab === "pages"}
    <button
      type="button"
      class="pages-fab"
      onclick={() => (mobileSheetOpen = !mobileSheetOpen)}
      aria-label={mobileSheetOpen ? "Hide learning pages" : "Show learning pages"}
      aria-expanded={mobileSheetOpen}
      aria-controls="paper-pages-sidebar"
      bind:this={pagesFab}
    >
      <span class="fab-glyph" aria-hidden="true">
        <svg viewBox="0 0 18 18" width="18" height="18" fill="none">
          <path d="M5 3h6l3 3v9H5z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/>
          <path d="M11 3v4h4M7 10h5M7 13h4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
        </svg>
      </span>
      <span class="fab-label">
        {activePage ? activePage.title : "Pages"}
      </span>
      <span class="fab-meta">{readyCount}/{pageCount}</span>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="M4 6l4 4 4-4" stroke="currentColor" stroke-width="1.6" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>
  {/if}
</div>

<style>
  .layout {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr;
    height: calc(100vh - 56px);
    height: calc(100dvh - 56px);
    overflow: hidden;
    background:
      radial-gradient(900px 680px at 80% -8%, rgba(154, 107, 255, 0.14), transparent 58%),
      var(--bg-void);
  }

  .layout.mobile {
    grid-template-columns: 1fr;
  }

  /* Tabs other than "pages" hide the sidebar so the screen owns its
     full width. Mobile is unaffected — the sidebar lives in a sheet. */
  .layout.no-sidebar {
    grid-template-columns: 1fr;
  }

  .sidebar-loading {
    width: 280px;
    border-right: 1px solid var(--border);
    background: var(--bg-panel);
  }

  .main {
    position: relative;
    min-width: 0;
    height: 100%;
    overflow: hidden;
    /* Flex column lets the optional missing-weights banner take its
       natural height and the page viewer fill the remainder. Without
       this the banner would push the viewer past the bottom edge. */
    display: flex;
    flex-direction: column;
  }
  .main > :global(section.viewer) {
    flex: 1;
    min-height: 0;
  }
  .main > :global(.tab) {
    flex: 1;
    min-height: 0;
  }

  .tab-panel {
    display: flex;
    flex: 1;
    min-height: 0;
    min-width: 0;
    outline: none;
  }

  .tab-panel > :global(*) {
    flex: 1;
    min-width: 0;
  }

  .tab-panel.has-mobile-fab > :global(section.viewer) {
    padding-bottom: max(74px, calc(60px + env(safe-area-inset-bottom)));
    box-sizing: border-box;
  }

  /* Tab strip: chip-style buttons, violet accent for active tab. Stays
     horizontal at narrow widths so the experiments tab is reachable
     even when the sidebar is hidden behind the FAB. */
  .tab-strip {
    display: flex;
    gap: 4px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.035), transparent 30%),
      rgba(9, 8, 18, 0.95);
    flex: none;
    /* Stay horizontal on narrow viewports — overflow scrolls instead of
       wrapping so the strip keeps its single-row chrome silhouette. */
    overflow-x: auto;
    overflow-y: hidden;
    flex-wrap: nowrap;
    scrollbar-width: none;
  }
  .tab-strip::-webkit-scrollbar { display: none; }
  .tab-btn { flex: none; white-space: nowrap; }

  .tab-btn {
    min-height: 36px;
    padding: 6px 14px;
    border-radius: var(--radius-pill);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-mid);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition:
      color var(--dur) var(--ease),
      background var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }

  .tab-btn:hover:not(.active) {
    color: var(--text-hi);
    background: rgba(154, 107, 255, 0.06);
  }

  .tab-btn.active {
    color: var(--text-hi);
    background:
      linear-gradient(120deg, rgba(255, 255, 255, 0.06), transparent 42%),
      rgba(154, 107, 255, 0.16);
    border-color: var(--violet-400);
    box-shadow:
      0 0 0 1px rgba(130, 71, 255, 0.45) inset,
      0 0 18px -12px var(--violet-glow);
  }

  .tab-btn:focus-visible,
  .pages-fab:focus-visible,
  .btn:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }

  .fatal {
    margin: auto;
    max-width: 440px;
    padding: 40px;
    text-align: center;
    color: var(--text-mid);
  }

  .fatal h2 {
    margin: 0 0 8px;
    color: var(--text-hi);
    font-size: var(--text-lg);
  }

  .fatal p {
    margin: 0 0 16px;
    font-size: var(--text-sm);
  }

  .btn {
    padding: 8px 16px;
    border-radius: var(--radius-pill);
    background: var(--violet-500);
    color: white;
    font-size: var(--text-sm);
    transition: transform var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
  }

  .btn:hover {
    transform: translateY(-1px);
    box-shadow: 0 8px 20px -8px rgba(106, 45, 255, 0.7);
  }

  /* Floating "Pages" trigger pinned above the iOS home bar. Tapping
     opens / closes the bottom-sheet sidebar. */
  .pages-fab {
    position: fixed;
    left: 14px;
    right: 14px;
    bottom: max(14px, env(safe-area-inset-bottom));
    z-index: calc(var(--z-drawer) + 1);
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.055), transparent 32%),
      var(--bg-glass);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    color: var(--text-hi);
    box-shadow: 0 18px 40px -16px rgba(106, 45, 255, 0.5);
    font-size: var(--text-sm);
    text-align: left;
    min-height: 48px;
  }
  .pages-fab:active { transform: translateY(1px); }
  .fab-glyph {
    display: grid;
    place-items: center;
    color: var(--violet-100);
  }
  .fab-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .fab-meta {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--violet-200);
    padding: 2px 8px;
    border-radius: var(--radius-pill);
    background: rgba(154, 107, 255, 0.12);
    border: 1px solid var(--border);
  }
</style>
