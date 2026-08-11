# Caveman Agent Prompt Templates

Compressed templates for dispatching Claude Code subagents. Strips
filler, keeps signal. ~60-70% fewer tokens vs. prose-heavy prompts.

## Translation rules

KEEP (unambiguous + machine-actionable):
- File paths
- Command names + exact arg shapes
- Schema bodies (Rust enums, TS types, SQL DDL)
- Specific imperatives (DO, DONT TOUCH, VERIFY)
- Field names + types when contracts cross boundaries

COMPRESS:
- Framing ("You are building X for Y") → cut
- "Context — read first" → just list paths
- Reasoning prose ("we want this because...") → trim to one fragment
- Multi-paragraph background → bulleted facts
- "Look at how agent X did it" → just point to the file

DROP:
- Encouragement, hedging, "if you can"
- Restatement of the goal in the middle of the prompt
- Coordination warnings already in the system prompt
- "Bonus points" / aspirational language

KEEP NORMAL (per caveman mode rules):
- Code blocks, schemas, commit messages, security notes

---

## Template — Implementation agent

```
GOAL: <one sentence>
READ:
  - docs/<design-doc>.md (sections X, Y)
  - <existing file path 1>
  - <existing file path 2>

DO:
  1. <imperative + concrete output>
  2. <imperative + concrete output>
  3. <imperative + concrete output>

CONTRACT (must match exactly):
  - <function/command name + signature>
  - <event payload shape>
  - <SQL DDL or schema>

DONT TOUCH:
  - <path>     # other agent owns this
  - <path>

VERIFY (must all pass):
  - <cmd 1>
  - <cmd 2>

REPORT (≤200 words):
  - files added/edited
  - <specific question, e.g. "did python3 detect work">
  - all gates green: y/n
```

---

## Template — Refactor / cleanup agent

```
GOAL: <what + why in one fragment>
SCOPE: <files or area>

CHANGES:
  1. <thing>: <from> → <to>
  2. <thing>: <from> → <to>

OUT OF SCOPE: <files / behavior to leave alone>

VERIFY:
  - <cmd>
  - existing tests pass

REPORT (≤150 words):
  - per change: 1-line summary + file:line
  - check sweep status
```

---

## Template — Code review agent

```
TARGET: <feature name + commit/branch or list of changed files>
FOCUS:
  1. <risk area + question>
  2. <risk area + question>
  3. <risk area + question>

READ:
  - docs/plans/<design-doc>.md
  - <key changed files>
  - CLAUDE.md (file-size caps + workaround sunset rules)

VERIFY:
  - <test cmds>

NO CODE EDITS.

REPORT (≤700 words, caveman output format):
  - what tested
  - what found (file:line)
  - what NOT tested
  - biggest risk
  - recommended next action
  - 🟢 solid · 🟡 concerns · 🔵 nits · 🔴 bugs
```

---

## Template — Test author agent

```
GOAL: cover <area> with <type> tests
EXISTING TESTS: <paths>

ADD:
  - <test name>: <what it asserts>
  - <test name>: <what it asserts>

CONSTRAINTS:
  - no new prod deps
  - no source code changes outside <test dir>
  - reuse existing test harness (<path>)

VERIFY:
  - <cmd>

REPORT: per-test 1-liner + pass status
```

---

## Worked example — verbose vs caveman

### Verbose (1850 tokens)

> You are building the **frontend UI** for the new "Experiment" feature
> in Smilie at `<repo-root>`. SvelteKit +
> Svelte 5 (runes) + TypeScript.
>
> ## CONTEXT — read first
>
> - `docs/plans/2026-04-26-experiments-design.md` — design contract.
>   Sections §4 (input/output schemas), §5 (host ↔ iframe ↔ worker
>   postMessage protocol — what your iframe code talks to), §7 (UI
>   layout).
> - `src/lib/ipc/index.ts` — the IPC contract is **pre-wired**. ...
> - `src/lib/components/PageViewer.svelte` is the existing
>   iframe-rendering component for learning pages — read it to
>   understand how the night-Tokyo theme + sandbox iframe pattern
>   works. Match the visual style.
> ...
> ### 1. Tab strip on paper detail
>
> The page currently renders `<Sidebar>` + `<PageViewer>` for the
> learning-pages flow. Add a **tab strip** at the top of the `<main>`
> area (above PageViewer) with two tabs: `Pages` (current default)
> and `Experiments`. ...

### Caveman (550 tokens)

```
GOAL: ship Experiments tab UI on paper detail page.
READ:
  - docs/plans/2026-04-26-experiments-design.md §4 §5 §7
  - src/lib/ipc/index.ts (pre-wired — DO NOT EDIT)
  - src/lib/components/PageViewer.svelte (style reference)
  - src/lib/stores/{toast,viewport,drawer}.ts

DO:
  1. tab strip on src/routes/papers/[id]/+page.svelte
     swaps <PageViewer/> ↔ <ExperimentTab paperId={paperId}/>
  2. src/lib/stores/experiments.ts — reactive store keyed by paperId.
     subscribes to events.onExperiment.
  3. components (all new):
     - ExperimentTab — left rail (list+New) + right pane
     - ExperimentRunner — header, plausibility chip, version
       breadcrumb, sandbox iframe, variable controls, run/refine/
       delete toolbar, outputs, run history (last 5), status banner
     - ExperimentVariableControls — auto-render InputSpec[] →
       range/select/textarea/vector. bind:values
     - ExperimentOutputs — typed render: number→chip, text→pre,
       table→html, series→tiny svg line chart (match Smilie.plot)
     - ExperimentVersionList — flat parent-chain rail
     - RefineExperimentDialog — textarea + inherit-inputs toggle +
       Fork primary. calls ipc.refineExperiment.

  4. iframe src: convertFileSrc(resourceDir + experiment-runner.html,
     "asset"). protocol per design §5.

CONTRACT (already in ipc/index.ts):
  ExperimentSummary, ExperimentDetail, ExperimentRun, InputSpec,
  OutputSpec, ExperimentEventPayload + ipc.{analyze,list,get,refine,
  delete,record,listRuns}Experiment*.

DONT TOUCH:
  - any *.rs / Cargo.toml / tauri.conf.json
  - src-tauri/assets/* (other agent)
  - src/lib/ipc/index.ts

CONSTRAINTS:
  - night-tokyo tokens (var(--violet-100), var(--bg-panel), …)
  - ≤500 lines/component
  - gate Tauri calls behind isTauri()

VERIFY:
  - npx svelte-check 0/0
  - npm run lint clean
  - npm test 17/17
  - npm run build clean

REPORT (≤300 words):
  - new components + new store
  - tab strip integration: file:line
  - iframe URL resolution choice
  - check sweep status
```

Same information, ~70% fewer tokens. Agent gets a denser brief, less
to scan, fewer attentional misses.

---

## When NOT to caveman the prompt

- Brainstorming agents (need conversational context to ask good
  questions back)
- Agents whose job is to write user-facing prose (their tone is the
  output; don't constrain it upstream)
- One-shot exploratory queries where you don't yet know the right
  imperative

For everything else — implementation, refactor, review, test — caveman
templates win.
