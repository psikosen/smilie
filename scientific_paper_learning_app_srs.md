# Software Requirements Specification

# Scientific Paper to Interactive Learning Pages App

## 1. Purpose

This document defines the requirements for a cross-platform native application that allows users to upload scientific papers, extract their content using GLM-OCR 0.9B, and automatically generate persistent interactive learning pages. These pages may include explanations, diagrams, quizzes, formula explorers, simulations, tables, figure breakdowns, and informational pages when interactivity is not useful.

The goal is to turn a dense scientific paper into a saved, navigable, playable learning experience.

---

## 2. Product Vision

The application should behave like a local AI-powered research tutor and learning-page generator.

A user uploads a scientific paper. The app reads the paper, extracts its text, equations, figures, tables, and structure, then creates a set of persistent pages that the user can revisit. Each page should teach one concept or section of the paper. When possible, the page should include interactive components. When interactivity is not appropriate, the page should be clean, informational, and easy to understand.

The app should work across desktop and mobile, with model selection depending on platform.

---

## 3. Target Platforms

### 3.1 Mobile

Mobile devices should use:

- Model: `google/gemma-4-E4B-it`
- Role: mobile reasoning, summarization, lightweight multimodal support, and optional local page generation
- Expected usage: shorter papers, summaries, concept pages, quizzes, lightweight visual explanations

### 3.2 Windows / Linux PC

PC systems with NVIDIA GPUs should use:

- Model: `Qwen3.6-35B-A3B-NVFP4`
- Role: main high-performance page generation, reasoning, code generation, and long-context paper analysis
- Target hardware: NVIDIA RTX 5090 32GB or similar Blackwell GPU
- Runtime: vLLM, SGLang, or another NVFP4-capable inference backend

### 3.3 macOS Native

macOS systems should use:

- Runtime: `mlx-vlm`
- Precision: BF16 Original model where available
- Role: native Apple Silicon VLM inference, multimodal paper/page reasoning, and local generation
- Target hardware: Apple Silicon Macs with sufficient unified memory

---

## 4. Core Model Architecture

### 4.1 OCR / Document Understanding Layer

Primary OCR model:

- `GLM-OCR 0.9B`

Responsibilities:

- Read uploaded scientific papers
- Extract page text
- Extract headings and section hierarchy
- Extract equations as LaTeX when possible
- Extract tables as structured JSON/Markdown
- Extract figure captions
- Detect figures, diagrams, charts, tables, formulas, and references
- Preserve page numbers and bounding boxes
- Produce a normalized paper representation

Output format:

```json
{
  "paper_id": "uuid",
  "title": "string",
  "authors": ["string"],
  "sections": [],
  "figures": [],
  "tables": [],
  "equations": [],
  "references": [],
  "pages": []
}
```

### 4.2 Reasoning / Planning Layer

The reasoning model should transform the OCR output into a learning plan.

Responsibilities:

- Summarize the paper
- Identify key concepts
- Identify prerequisites
- Identify equations worth explaining
- Identify figures worth making interactive
- Identify possible simulations
- Decide which pages should be interactive
- Decide which pages should be informational
- Create a page dependency graph

Example output:

```json
{
  "learning_plan": {
    "pages": [
      {
        "title": "Main Claim",
        "type": "informational",
        "source_sections": ["abstract", "introduction"]
      },
      {
        "title": "Equation Explorer",
        "type": "interactive_formula",
        "source_sections": ["methods"]
      }
    ]
  }
}
```

### 4.3 Page Generation Layer

The page generation model should create durable learning pages.

Responsibilities:

- Generate HTML/CSS/JS learning pages
- Generate interactive widgets when useful
- Generate quizzes
- Generate glossary pages
- Generate diagrams
- Generate formula explorers
- Generate chart/table explorers
- Generate source-grounded explanations
- Include paper citations or section references internally
- Save pages persistently

Preferred page format:

- Self-contained HTML modules
- Local JavaScript only
- No remote CDN dependency required
- Optional component wrapper for React/Svelte/Tauri WebView

---

## 5. Application Architecture

## 5.1 Recommended Stack

### Desktop Native App

Preferred:

- Tauri v2
- Frontend: Svelte, React, or Solid
- Backend: Rust commands + Python sidecar for ML tasks
- Local database: SQLite
- Local asset store: app data directory

Alternative:

- Flutter desktop/mobile app
- Python/FastAPI local service
- SQLite or Isar database

### Mobile App

Preferred:

- Flutter
- Local model bridge for Gemma 4 E4B where supported
- Optional server/local-network desktop companion for heavier generation

### macOS Native

Preferred:

- Tauri or SwiftUI shell
- `mlx-vlm` Python sidecar or native bridge
- Local Apple Silicon inference
- add InternVL3-8B as another option for the vision model

---

## 6. High-Level System Flow

```text
User uploads PDF
    ↓
PDF is stored locally
    ↓
Pages are rendered to images if needed
    ↓
GLM-OCR 0.9B extracts text/layout/tables/formulas/figures
    ↓
Normalized paper JSON is saved
    ↓
Reasoning model creates learning plan
    ↓
Generator model creates pages
    ↓
Pages are validated and sandboxed
    ↓
Pages are saved permanently
    ↓
User opens pages from left sidebar
```

---

## 7. User Interface Requirements

## 7.1 Main Layout

The app shall use a simple two-panel layout.

```text
--------------------------------------------------
| Left Retractable List | Right Learning Page     |
|-----------------------|-------------------------|
| Paper Title           | Interactive component   |
| - Summary             | Explanation             |
| - Key Concepts        | Quiz                    |
| - Methods             | Diagram                 |
| - Figure Explorer     | Simulation              |
| - Formula Explorer    |                         |
| - Glossary            |                         |
--------------------------------------------------
```

### Required UI components

- Upload button
- Paper library view
- Retractable left sidebar
- Separator/resizer between sidebar and page view
- Search within generated pages
- Page status indicator
- Generation progress view
- Settings page
- Model selection page
- Regenerate page button
- Export page button
- Delete paper button

---

## 8. Persistent Page Requirements

Each generated page shall be saved permanently unless deleted by the user.

Each page record shall include:

- Page ID
- Paper ID
- Page title
- Page type
- Source sections
- Source page numbers
- Generated HTML
- Generated CSS
- Generated JS
- Asset references
- Creation timestamp
- Last updated timestamp
- Model used
- Generation prompt hash
- Validation status

Pages shall be viewable offline after generation.

---

## 9. Page Types

The app shall support the following page types.

### 9.1 Informational Page

Used when no useful interactivity is available.

Examples:

- Summary
- Background
- Study motivation
- Limitations
- Discussion

### 9.2 Concept Explainer

Explains a single scientific concept.

Required elements:

- Plain-language explanation
- Technical explanation
- Key terms
- Source-backed notes
- Mini quiz

### 9.3 Formula Explorer

Used for equations.

Possible controls:

- Sliders for variables
- Live formula output
- Graph area
- Units explanation
- Step-by-step derivation

### 9.4 Figure Explorer

Used for charts, diagrams, or scientific figures.

Possible controls:

- Labels that toggle on/off
- Caption simplifier
- Highlight regions
- Explain axes
- Data extraction when possible

### 9.5 Table Explorer

Used for tables.

Possible controls:

- Sort
- Filter
- Highlight significant values
- Convert table to chart
- Explain columns

### 9.6 Quiz Page

Used for reinforcement.

Question types:

- Multiple choice
- True/false
- Fill in the blank
- Matching
- Explain in your own words

### 9.7 Simulation Page

Used when a mechanism, model, process, or algorithm can be made interactive.

Examples:

- Parameter sliders
- Animated flow
- Process diagram
- Mini model demo
- Cause/effect explorer

---

## 10. Model Selection Requirements

## 10.1 Model Profiles

The app shall include model profiles.

```json
{
  "profiles": [
    {
      "name": "Mobile Gemma 4 E4B",
      "platform": "mobile",
      "model_id": "google/gemma-4-E4B-it",
      "roles": ["summary", "light_reasoning", "mobile_page_generation"]
    },
    {
      "name": "PC Qwen3.6 35B A3B NVFP4",
      "platform": "windows_linux_pc",
      "model_id": "Qwen3.6-35B-A3B-NVFP4",
      "roles": ["deep_reasoning", "long_context", "page_generation", "code_generation"]
    },
    {
      "name": "macOS MLX-VLM BF16",
      "platform": "macos",
      "runtime": "mlx-vlm",
      "precision": "BF16 Original",
      "roles": ["multimodal_reasoning", "local_generation", "document_understanding"]
    }
  ]
}
```

## 10.2 Settings Page

The settings page shall allow the user to configure:

- OCR model path
- Reasoning model
- Page generation model
- Runtime backend
- Context length
- Temperature
- Max tokens
- Thinking mode on/off if supported
- GPU memory utilization
- Output page style
- Interactive page aggressiveness
- Offline-only mode

## 10.3 Model Fallback

If a selected model fails, the app shall:

1. Preserve the failed generation state
2. Show a readable error
3. Allow retry
4. Allow model switch
5. Resume from the last completed pipeline stage

---

## 11. Document Processing Requirements

The app shall support:

- PDF upload
- Scientific paper PDFs
- Scanned PDFs
- Born-digital PDFs
- Multi-column papers
- Figures and captions
- Equations
- Tables
- References
- Supplementary PDFs if added later

The app should preserve source references so users can trace generated explanations back to the paper.

---

## 12. Generation Pipeline Requirements

## 12.1 Pipeline Stages

The app shall use resumable pipeline stages.

```text
1. import_pdf
2. render_pages
3. ocr_extract
4. structure_paper
5. build_learning_plan
6. generate_pages
7. validate_pages
8. save_pages
9. index_pages
10. complete
```

Each stage shall be logged.

## 12.2 Background Generation

The app shall generate pages in the background.

Requirements:

- User can close the paper view while generation continues
- User can pause generation
- User can cancel generation
- User can resume failed generation
- User can view partially completed pages
- App must not lose progress after restart

## 12.3 Job Queue

The app shall maintain a local job queue.

Job fields:

- Job ID
- Paper ID
- Stage
- Status
- Progress percentage
- Current model
- Error message
- Created at
- Updated at
- Retry count

---

## 13. Database Requirements

Recommended database:

- SQLite

Core tables:

```sql
papers
pages
assets
models
jobs
ocr_blocks
figures
tables
equations
settings
generation_logs
```

### 13.1 Papers Table

Stores uploaded papers.

Fields:

- id
- title
- authors
- file_path
- original_filename
- page_count
- created_at
- updated_at
- processing_status

### 13.2 Pages Table

Stores generated learning pages.

Fields:

- id
- paper_id
- title
- type
- html_path
- source_json
- model_id
- status
- created_at
- updated_at

### 13.3 OCR Blocks Table

Stores extracted document blocks.

Fields:

- id
- paper_id
- page_number
- block_type
- text
- bbox
- confidence
- source_model

---

## 14. Security Requirements

Generated pages shall be sandboxed.

Requirements:

- No arbitrary filesystem access from generated pages
- No network access unless explicitly allowed
- No remote script injection by default
- Generated JS must run inside a restricted WebView/sandbox
- User-uploaded papers remain local unless cloud mode is added later
- Model files should be loaded only from user-approved paths

---

## 15. Validation Requirements

Generated pages shall be validated before being saved as complete.

Validation checks:

- HTML parses correctly
- CSS is valid enough to render
- JavaScript does not contain blocked APIs
- Page references valid source sections
- Page has title and type
- Page does not claim unsupported facts without source text
- Interactive components do not crash during smoke test

---

## 16. Non-Functional Requirements

## 16.1 Performance

- OCR should process pages incrementally.
- Users should see progress within a few seconds of upload.
- Generated pages should open quickly after creation.
- The app should support partial viewing before all pages are complete.

## 16.2 Reliability

- Pipeline must be resumable.
- App restart must not lose progress.
- Failed pages can be regenerated individually.
- Original OCR output must be preserved.

## 16.3 Offline Use

The app should support full offline use after models are downloaded.

## 16.4 Portability

The core learning-page format should be portable across platforms.

---

## 17. MVP Scope

The first version shall include:

- PDF upload
- GLM-OCR 0.9B extraction
- Paper library
- Left sidebar navigation
- Right page viewer
- Persistent generated pages
- Informational pages
- Concept explainer pages
- Basic quizzes
- Formula explorer for simple equations
- Model settings
- Local SQLite storage
- Background generation queue

---

## 18. Post-MVP Features

Future features may include:

- Voice tutor mode
- Spaced repetition cards
- Export to Anki
- Export to static website
- Export to PDF study guide
- Multi-paper comparison
- Citation graph explorer
- Figure-to-animation generation
- Collaborative paper notebooks
- Cloud sync
- Mobile companion mode
- Research collection manager
- Paper recommendation engine

---

## 19. Acceptance Criteria

The MVP is complete when:

1. A user can upload a scientific PDF.
2. GLM-OCR 0.9B extracts the paper into structured JSON.
3. The app creates a learning plan.
4. The app generates at least five persistent learning pages.
5. The left sidebar lists the generated pages.
6. Clicking a page opens it on the right.
7. At least one generated page can be interactive.
8. Informational pages are created when no useful interaction exists.
9. Pages remain available after app restart.
10. The user can change model settings.
11. The app can resume after a failed generation job.

---

## 20. Recommended Build Order

### Phase 1: Shell and Storage

- Build native shell
- Add SQLite
- Add paper upload
- Add paper library
- Add left sidebar/right viewer layout

### Phase 2: OCR Pipeline

- Add PDF rendering
- Add GLM-OCR 0.9B integration
- Save OCR blocks
- Save normalized paper JSON

### Phase 3: Learning Plan Generator

- Add model runtime abstraction
- Generate paper summary
- Generate section map
- Generate page plan

### Phase 4: Page Generator

- Generate informational pages
- Generate concept pages
- Generate quiz pages
- Generate formula pages
- Save pages persistently

### Phase 5: Background Jobs

- Add job queue
- Add progress view
- Add pause/cancel/resume
- Add retry and error recovery

### Phase 6: Platform Model Profiles

- Add mobile Gemma 4 E4B profile
- Add PC Qwen3.6-35B-A3B-NVFP4 profile
- Add macOS MLX-VLM BF16 profile
- Add custom model path support

### Phase 7: Polish

- Add page search
- Add export
- Add regenerate page
- Add safety validation
- Add settings UI

---

## 21. Key Design Principle

The app should not merely summarize papers.

It should transform papers into durable learning objects.

The best version of the app feels like this:

```text
A paper goes in.
A mini interactive textbook comes out.
```

