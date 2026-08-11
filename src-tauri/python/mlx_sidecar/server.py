"""Smilie mlx-vlm sidecar.

Newline-delimited JSON-RPC over stdin/stdout. See
``src-tauri/src/runtime/mlx_vlm.rs`` for the Rust-side protocol contract.

Requests:
    {"id": "<id>", "method": "ocr" | "reason" | "generate"
                          | "analyze_experiment" | "generate_experiment"
                          | "refine_experiment", "params": {...}}

Responses:
    {"id": "<id>", "ok": true, "result": {...}}
    {"id": "<id>", "ok": false, "error": "..."}

If weights are missing, the error string is prefixed with
``missing_weights:<path>`` so the Rust side can surface the typed
``RuntimeError::MissingWeights`` variant to the UI.

This file is intentionally conservative: we only import mlx-vlm lazily,
so the sidecar boots and reports MissingWeights cleanly even when the
library is not installed yet.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import traceback
from pathlib import Path
from typing import Any


def log(msg: str) -> None:
    print(f"[mlx_sidecar] {msg}", file=sys.stderr, flush=True)


class Sidecar:
    def __init__(self, model_path: Path) -> None:
        self.model_path = model_path
        self._model = None
        self._processor = None

    def _ensure_model(self) -> None:
        if self._model is not None:
            return
        if not self.model_path.exists():
            raise _MissingWeights(str(self.model_path))

        try:
            from mlx_vlm import load  # type: ignore

            log(f"loading model: {self.model_path}")
            self._model, self._processor = load(str(self.model_path))
        except ImportError as e:
            raise RuntimeError(
                "mlx-vlm not installed in the sidecar environment. "
                "Install with: pip install mlx-vlm"
            ) from e

    def ocr(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        image_path = params["path"]
        page_number = int(params.get("page_number", 1))

        # GLM-OCR is prompt-limited. Its official document-recognition
        # contract uses this exact task prompt; longer custom JSON prompts
        # can produce coordinates or incomplete text instead of OCR.
        prompt = "Text Recognition:"

        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [image_path],
            max_tokens=8192,
        )

        items = _parse_blocks_lenient(raw)
        blocks = [_normalize_ocr_block(b, page_number) for b in items]
        # Drop anything that ended up empty after normalization.
        blocks = [b for b in blocks if b is not None]
        return {"blocks": blocks}

    def reason(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        paper = params
        # /no_think disables Qwen3's CoT prelude — the reason endpoint
        # only needs the structured plan, not a thinking trace, and the
        # prelude routinely consumed the entire token budget before
        # the JSON was emitted. The token budget is also higher than
        # needed (12k) to absorb verbose plans without truncation.
        prompt = (
            "/no_think\n"
            "You are a research tutor. Given this scientific paper JSON, "
            "produce a learning plan with 6-10 pages. Each page has: "
            "title, type (one of: informational, concept, formula, figure, "
            "table, quiz, simulation), source_sections (list of section ids), "
            "source_pages (list of page numbers), source_content (the most "
            "relevant extracted text snippets for this page). "
            "Respond with EXACTLY one JSON object and nothing else: "
            '{"summary": "...", "pages": [ ... ]}. '
            "No prose. No thinking. No code fences. Just the JSON.\n\n"
            f"PAPER JSON:\n{json.dumps(paper)[:120000]}"
        )

        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [],
            max_tokens=12288,
            enable_thinking=False,
        )

        plan = _parse_json_object(raw)
        if "pages" not in plan:
            raise RuntimeError(f"reasoning model returned malformed plan: {raw[:200]}")
        return _normalize_plan(plan)

    def generate(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        spec = params
        # Reading-level branching: the host (Rust) optionally tags the
        # spec with `level` ∈ {eli5, default, expert}. The schema +
        # block shapes don't change; we just steer the audience tone via
        # an extra prompt prefix so the same source content can be
        # re-rendered at a different level without losing the structure.
        # `level` may be missing, null, or a non-string from a flaky
        # caller. Coerce to a string before the dict lookup so Pyright
        # is happy AND we don't blow up on garbage input.
        raw_level = spec.get("level") if isinstance(spec, dict) else None
        level = raw_level if isinstance(raw_level, str) else ""
        level_prefix = _LEVEL_PROMPT_PREFIX.get(level, "")
        prompt = level_prefix + _BLOCKS_PROMPT.format(spec=json.dumps(spec))

        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [],
            max_tokens=6144,
            enable_thinking=False,
        )

        try:
            obj = _parse_json_object(raw)
        except RuntimeError:
            cleaned = _strip_code_fences(raw)
            repaired = _repair_truncated_json(cleaned)
            obj = _try_load(repaired) if repaired else None
            if not isinstance(obj, dict):
                raise RuntimeError(
                    f"generator did not return a JSON page object: {raw[:200]}"
                )

        blocks = _normalize_page_blocks(obj, fallback_title=spec.get("title", ""))

        return {
            "blocks": blocks,
            "source_json": raw,
            "model_id": self.model_path.name,
        }

    # ── Experiments ────────────────────────────────────────────
    #
    # Three short methods that drive the experiment pipeline. Each is
    # constrained to JSON-only output, /no_think (Qwen) so the CoT
    # prelude doesn't eat the token budget, and routed through the
    # tolerant `_parse_json_object` + `_repair_truncated_json` helpers.

    def analyze_experiment(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        # Trim the paper down to the bits a reproducibility verdict
        # actually needs: title, authors, abstract / first-section text,
        # and the heading + first paragraph of each remaining section.
        # Skips equations / figure captions / refs. Roughly 5-10k chars
        # vs the prior 120k → 5-10× faster prefill on Apple Silicon.
        paper_blob = _slim_paper_for_analyze(params)
        prompt = (
            "/no_think\n"
            "Job: judge if Python (numpy+scipy, <200 lines) reproduce paper main claim. "
            "Score 0-100. <50 = no. 50 = maybe. >50 = yes.\n"
            "Respond ONE JSON object only. No prose. No thinking. No fences.\n"
            'Shape: {"plausible": true|false, "score": <int 0..100>, '
            '"reasoning": "...", "approach_summary": "...", '
            '"suggested_title": "..."}\n\n'
            f"PAPER:\n{paper_blob}"
        )
        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [],
            max_tokens=1024,
            enable_thinking=False,
        )
        obj = _parse_experiment_json(raw)
        return _normalize_experiment_analysis(obj)

    def generate_experiment(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        paper = params.get("paper", {})
        approach = params.get("approach", "")
        paper_blob = json.dumps(paper)[:80_000]

        prompt = _EXPERIMENT_GENERATE_PROMPT.format(
            approach=approach, paper=paper_blob
        )
        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [],
            max_tokens=8192,
            enable_thinking=False,
        )
        obj = _parse_experiment_json(raw)
        artifacts = _normalize_experiment_artifacts(obj)
        # Tests must be `def test_*():` functions to match the Pyodide
        # worker harness (see experiment-worker.js). If the model
        # produced bare top-level asserts anyway, do ONE retry asking
        # explicitly for the wrapped form.
        if not _has_test_functions(artifacts.get("python_tests", "")):
            retry_prompt = _EXPERIMENT_RETRY_TESTS_PROMPT.format(
                code=artifacts.get("python_code", ""),
                tests=artifacts.get("python_tests", ""),
            )
            retry_raw = _mlx_generate(
                self._model,
                self._processor,
                retry_prompt,
                [],
                max_tokens=4096,
                enable_thinking=False,
            )
            try:
                retry_obj = _parse_experiment_json(retry_raw)
                retry_artifacts = _normalize_experiment_artifacts(retry_obj)
                if _has_test_functions(retry_artifacts.get("python_tests", "")):
                    artifacts = retry_artifacts
            except RuntimeError:
                # Retry parse failed — fall through with original
                # artifacts; the host-side validator will surface a
                # clear test-shape error in the refine loop.
                pass
        return artifacts

    def refine_experiment(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        paper = params.get("paper", {})
        prior = params.get("prior", {})
        paper_blob = json.dumps(paper)[:60_000]
        user_prompt = prior.get("user_refine_prompt")
        if user_prompt:
            directive = (
                f"USER REFINE PROMPT (apply verbatim, prefer code changes "
                f"over test changes):\n{user_prompt}\n"
            )
        else:
            directive = "Fix the code so the tests pass. DO NOT modify the tests.\n"

        prompt = _EXPERIMENT_REFINE_PROMPT.format(
            directive=directive,
            code=prior.get("code", ""),
            tests=prior.get("tests", ""),
            error_output=prior.get("error_output", ""),
            paper=paper_blob,
        )
        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [],
            max_tokens=8192,
            enable_thinking=False,
        )
        obj = _parse_experiment_json(raw)
        artifacts = _normalize_experiment_artifacts(obj)
        # Same one-shot retry guard as generate_experiment — the runner
        # contract is `def test_*():` functions, full stop.
        if not _has_test_functions(artifacts.get("python_tests", "")):
            retry_prompt = _EXPERIMENT_RETRY_TESTS_PROMPT.format(
                code=artifacts.get("python_code", ""),
                tests=artifacts.get("python_tests", ""),
            )
            retry_raw = _mlx_generate(
                self._model,
                self._processor,
                retry_prompt,
                [],
                max_tokens=4096,
                enable_thinking=False,
            )
            try:
                retry_obj = _parse_experiment_json(retry_raw)
                retry_artifacts = _normalize_experiment_artifacts(retry_obj)
                if _has_test_functions(retry_artifacts.get("python_tests", "")):
                    artifacts = retry_artifacts
            except RuntimeError:
                pass
        return artifacts


    # ── Two-paper comparison ───────────────────────────────────
    #
    # Produces a PageBlocks JSON object diffing two papers — same shape
    # the generator emits for a learning page, so the existing Rust
    # renderer (`pipeline::render`) handles styling identically. Asks
    # the model to surface shared concepts, contradictions, unique
    # contributions, and an optional `diagram_flow` overlay.

    def compare_papers(self, params: dict[str, Any]) -> dict[str, Any]:
        self._ensure_model()
        assert self._model is not None and self._processor is not None

        a = params.get("a", {})
        b = params.get("b", {})
        a_blob = json.dumps(a)[:60_000]
        b_blob = json.dumps(b)[:60_000]
        prompt = _COMPARE_PROMPT.format(a=a_blob, b=b_blob)
        raw = _mlx_generate(
            self._model,
            self._processor,
            prompt,
            [],
            max_tokens=8192,
            enable_thinking=False,
        )
        try:
            obj = _parse_json_object(raw)
        except RuntimeError:
            cleaned = _strip_code_fences(raw)
            repaired = _repair_truncated_json(cleaned)
            obj = _try_load(repaired) if repaired else None
            if not isinstance(obj, dict):
                raise RuntimeError(
                    f"compare_papers did not return a JSON object: {raw[:200]}"
                )
        a_title = a.get("title") if isinstance(a, dict) else ""
        b_title = b.get("title") if isinstance(b, dict) else ""
        fallback_title = f"Comparison: {a_title} vs {b_title}".strip()
        return _normalize_page_blocks(obj, fallback_title=fallback_title)


class _MissingWeights(Exception):
    def __init__(self, path: str) -> None:
        super().__init__(path)
        self.path = path


def _mlx_generate(
    model: Any,
    processor: Any,
    prompt: str,
    images: list[str],
    max_tokens: int,
    enable_thinking: bool | None = None,
) -> str:
    """Thin wrapper around mlx-vlm's ``generate`` that always returns a str.

    mlx-vlm's return type has shifted between releases (str vs.
    GenerationResult with `.text`). Normalize here so call sites don't care.
    Also handles the chat-template formatting which can return ``list`` or
    ``str`` depending on the processor.

    `enable_thinking` is forwarded to the tokenizer's chat template — Qwen3
    family models support this kwarg to disable the CoT prelude. We catch
    ``TypeError`` so unrelated tokenizers that ignore it don't break.
    """
    from mlx_vlm import generate as _gen  # type: ignore
    from mlx_vlm.prompt_utils import apply_chat_template  # type: ignore

    num_images = len(images)
    # mlx-vlm >= 0.1.x dereferences `config.__dict__` internally, so we MUST
    # pass the loaded model's config object (not None — that crashes with
    # `'NoneType' object has no attribute '__dict__'` deep inside the
    # template renderer).
    config = getattr(model, "config", None) or getattr(processor, "model_config", None)
    template_kwargs: dict[str, Any] = {"num_images": num_images}
    if enable_thinking is not None:
        template_kwargs["enable_thinking"] = enable_thinking
    try:
        formatted = apply_chat_template(processor, config, prompt, **template_kwargs)
    except TypeError:
        # Older tokenizer that doesn't recognize enable_thinking.
        template_kwargs.pop("enable_thinking", None)
        formatted = apply_chat_template(processor, config, prompt, **template_kwargs)
    prompt_str = formatted if isinstance(formatted, str) else json.dumps(formatted)

    result = _gen(
        model,
        processor,
        prompt_str,  # type: ignore[arg-type]
        images,
        max_tokens=max_tokens,
        verbose=False,
    )

    if isinstance(result, str):
        return result
    text = getattr(result, "text", None)
    if isinstance(text, str):
        return text
    return str(result)


_VALID_BLOCK_TYPES = {
    "text", "heading", "figure", "table",
    "equation", "caption", "reference",
}

_OCR_BLOCK_TYPE_ALIASES = {
    "header": "heading",
    "title": "heading",
    "paragraph": "text",
    "formula": "equation",
    "math": "equation",
    "display_math": "equation",
    "inline_math": "equation",
    "image": "figure",
    "bibliography": "reference",
}

_VALID_PAGE_TYPES = {
    "informational", "concept", "formula", "figure",
    "table", "quiz", "simulation",
}


def _as_string(v: Any) -> str:
    """Coerce to the shape Rust serde wants: a String. Joins lists with
    blank lines (snippets) and stringifies dicts/numbers."""
    if v is None:
        return ""
    if isinstance(v, str):
        return v
    if isinstance(v, list):
        return "\n\n".join(_as_string(x) for x in v if x is not None and x != "")
    if isinstance(v, (int, float, bool)):
        return str(v)
    if isinstance(v, dict):
        return json.dumps(v, ensure_ascii=False)
    return str(v)


def _as_string_list(v: Any) -> list[str]:
    if v is None:
        return []
    if isinstance(v, list):
        return [_as_string(x) for x in v if x is not None and x != ""]
    if isinstance(v, str):
        # Tolerate `"s1, s2"` and `"[s1, s2]"`.
        s = v.strip().strip("[]")
        if not s:
            return []
        return [p.strip().strip('"') for p in s.split(",") if p.strip()]
    return [_as_string(v)]


def _as_int_list(v: Any) -> list[int]:
    raw_items = _as_string_list(v)
    out: list[int] = []
    for item in raw_items:
        try:
            out.append(int(float(item)))
        except (TypeError, ValueError):
            continue
    return out


def _normalize_page_spec(p: Any) -> dict[str, Any]:
    """Coerce one plan-page entry into the exact shape Rust's PageSpec
    deserializer expects: title/source_content as strings, page_type
    as a constrained enum, source_sections as Vec<String>,
    source_pages as Vec<u32>, assets present (defaulted)."""
    if not isinstance(p, dict):
        p = {"title": _as_string(p), "type": "informational"}

    page_type = p.get("type") or p.get("page_type") or "informational"
    if page_type not in _VALID_PAGE_TYPES:
        page_type = "informational"

    assets = p.get("assets") or {}
    if not isinstance(assets, dict):
        assets = {}

    return {
        "title": _as_string(p.get("title")) or "Untitled",
        "type": page_type,
        "source_sections": _as_string_list(p.get("source_sections")),
        "source_pages": _as_int_list(p.get("source_pages")),
        "source_content": _as_string(p.get("source_content")),
        "assets": {
            "figures": _as_string_list(assets.get("figures")),
            "tables": _as_string_list(assets.get("tables")),
            "equations": _as_string_list(assets.get("equations")),
        },
    }


def _normalize_plan(plan: Any) -> dict[str, Any]:
    """Coerce the whole reason()-output into Rust LearningPlan shape."""
    if not isinstance(plan, dict):
        plan = {"pages": [], "summary": _as_string(plan)}
    pages_raw = plan.get("pages") or []
    if not isinstance(pages_raw, list):
        pages_raw = [pages_raw]
    return {
        "summary": _as_string(plan.get("summary")),
        "pages": [_normalize_page_spec(p) for p in pages_raw],
    }


def _normalize_ocr_block(b: Any, page_number: int) -> dict[str, Any] | None:
    """Coerce whatever the model produced into the shape the Rust
    `OcrBlock` struct expects.

    The model is unreliable about output shape — it sometimes returns
    strings instead of objects, sometimes nests `text` as a list, etc.
    Be liberal in what we accept; let downstream worry about meaning.
    """
    if isinstance(b, str):
        b = {"block_type": "text", "text": b}
    if not isinstance(b, dict):
        return None

    raw_type = _as_string(b.get("block_type") or b.get("type") or "text")
    raw_type = raw_type.strip().lower()
    block_type = _OCR_BLOCK_TYPE_ALIASES.get(raw_type, raw_type)
    if block_type not in _VALID_BLOCK_TYPES:
        block_type = "text"

    text = b.get("text")
    if block_type == "equation":
        text = b.get("latex") or b.get("equation") or text
    if isinstance(text, list):
        text = "\n".join(str(t) for t in text if t)
    elif text is not None and not isinstance(text, str):
        text = str(text)
    if block_type == "equation" and text:
        text = _clean_latex(text)

    out: dict[str, Any] = {
        "page_number": page_number,
        "block_type": block_type,
        "text": text,
    }

    bbox = b.get("bbox")
    if isinstance(bbox, list) and len(bbox) == 4:
        try:
            out["bbox"] = [float(x) for x in bbox]
        except (TypeError, ValueError):
            pass

    conf = b.get("confidence")
    if isinstance(conf, (int, float)):
        out["confidence"] = float(conf)

    return out


def _clean_latex(value: Any) -> str:
    """Normalize model wrappers without rewriting mathematical content."""
    latex = _strip_code_fences(_as_string(value)).strip()
    wrappers = (("$$", "$$"), (r"\[", r"\]"), (r"\(", r"\)"), ("$", "$"))
    for opening, closing in wrappers:
        if latex.startswith(opening) and latex.endswith(closing):
            inner = latex[len(opening):len(latex) - len(closing)].strip()
            if inner:
                return inner
    return latex


_DISPLAY_LATEX_RE = re.compile(
    r"\$\$(.+?)\$\$|\\\[(.+?)\\\]",
    re.DOTALL,
)


def _split_recognized_text(text: str) -> list[dict[str, str]]:
    """Promote display-math regions from GLM's text output to equations."""
    blocks: list[dict[str, str]] = []
    cursor = 0
    for match in _DISPLAY_LATEX_RE.finditer(text):
        prose = text[cursor:match.start()].strip()
        if prose:
            blocks.append({"block_type": "text", "text": prose})
        latex = _clean_latex(match.group(0))
        if latex:
            blocks.append({"block_type": "equation", "text": latex})
        cursor = match.end()

    remainder = text[cursor:].strip()
    if remainder:
        blocks.append({"block_type": "text", "text": remainder})
    return blocks or [{"block_type": "text", "text": text.strip()}]


_THINK_RE = re.compile(r"<think>.*?</think>", re.DOTALL | re.IGNORECASE)


def _strip_code_fences(s: str) -> str:
    """Drop ``<think>…</think>`` blocks (Qwen3 reasoning prelude),
    surrounding ```json fences, and dangling chat-template tokens."""
    s = _THINK_RE.sub("", s)
    s = s.strip()
    if s.startswith("```"):
        nl = s.find("\n")
        if nl >= 0:
            s = s[nl + 1 :]
        else:
            s = s.lstrip("`")
    if s.endswith("```"):
        s = s[:-3].rstrip()
    # Remove dangling chat tokens.
    for token in ("<|user|>", "<|assistant|>", "<|system|>", "<|endoftext|>"):
        if s.endswith(token):
            s = s[: -len(token)].rstrip()
    return s.strip()


def _try_load(s: str) -> Any:
    """json.loads or None — never raises."""
    try:
        return json.loads(s)
    except json.JSONDecodeError:
        return None


def _coerce_to_block_list(obj: Any) -> list[Any] | None:
    """Take whatever ``json.loads`` produced and squeeze a list-of-blocks
    out of it. Returns None if it can't be salvaged.

    Accepts:
      • list (best case)
      • {"blocks": [...]} / "items" / "data" / "result"
      • A bare single block: {"block_type": ..., "text": ...} → wrapped
      • A dict whose values are all blocks: {"0": {...}, "1": {...}} → values
    """
    if isinstance(obj, list):
        return obj
    if not isinstance(obj, dict):
        return None
    for key in ("blocks", "items", "data", "result", "pages"):
        v = obj.get(key)
        if isinstance(v, list):
            return v
    # Looks like a single block?
    if any(k in obj for k in ("block_type", "type", "text", "bbox")):
        return [obj]
    # Last shot: dict-of-blocks.
    vals = list(obj.values())
    if vals and all(isinstance(v, dict) for v in vals):
        return vals
    return None


def _parse_json_array(raw: str) -> list[Any]:
    """Pull a list-of-blocks out of the model's free-form output.

    GLM-OCR-0.9B is small and frequently:
      • wraps output in a ```json … ``` fence
      • returns a single object instead of an array
      • sneaks chat-template tokens onto the tail
      • emits commentary before the JSON
    We try increasingly aggressive strategies; raise only if all fail.
    """
    cleaned = _strip_code_fences(raw)

    # 1. Whole cleaned string parses cleanly.
    parsed = _try_load(cleaned)
    if parsed is not None:
        coerced = _coerce_to_block_list(parsed)
        if coerced is not None:
            return coerced

    # 2. Slice from first `[` to last `]`. Guards against bbox arrays
    #    inside dicts by requiring the slice itself to parse to a list
    #    of dicts.
    start = cleaned.find("[")
    end = cleaned.rfind("]")
    if start >= 0 and end > start:
        parsed = _try_load(cleaned[start : end + 1])
        coerced = _coerce_to_block_list(parsed) if parsed is not None else None
        if coerced and any(isinstance(b, (dict, str)) for b in coerced):
            return coerced

    # 3. Slice from first `{` to last `}`.
    o_start = cleaned.find("{")
    o_end = cleaned.rfind("}")
    if o_start >= 0 and o_end > o_start:
        parsed = _try_load(cleaned[o_start : o_end + 1])
        if parsed is not None:
            coerced = _coerce_to_block_list(parsed)
            if coerced is not None:
                return coerced

    raise RuntimeError(f"model did not return parseable JSON: {raw[:200]}")


def _repair_truncated_json(s: str) -> str | None:
    """Heuristically close truncated JSON the model didn't finish.

    Walks the string tracking string state, brackets, and braces; at
    EOF, closes whatever is still open. If the input ended inside an
    unterminated string, drops the partial value (and any trailing
    `:`/`,`) before closing brackets. Returns ``None`` only for input
    that is structurally beyond rescue (e.g. excess closers).
    """
    s = s.strip()
    if not s:
        return None

    in_string = False
    escape = False
    stack: list[str] = []
    open_string_at = -1  # index of the `"` that opened the still-open string

    for i, ch in enumerate(s):
        if escape:
            escape = False
            continue
        if in_string:
            if ch == "\\":
                escape = True
            elif ch == '"':
                in_string = False
                open_string_at = -1
            continue
        if ch == '"':
            in_string = True
            open_string_at = i
        elif ch in "{[":
            stack.append("}" if ch == "{" else "]")
        elif ch in "}]":
            if stack and stack[-1] == ch:
                stack.pop()
            else:
                return None  # malformed beyond rescue

    # If we're inside an unterminated string at EOF, lop it off
    # entirely (along with the `:` or `,` that introduced it) so we
    # don't keep a half-key/value around.
    if in_string and open_string_at >= 0:
        cut = open_string_at - 1
        # Skip whitespace and the introducer character.
        while cut >= 0 and s[cut] in " \t\n\r":
            cut -= 1
        if cut >= 0 and s[cut] in ":,":
            cut -= 1
        # Skip an optional key (string + whitespace) preceding a `:`.
        # Pattern: `"key":"unfinished value` → drop the key too.
        if cut >= 0 and s[cut] == '"':
            j = cut - 1
            while j >= 0 and s[j] != '"':
                if s[j] == "\\":
                    j -= 2
                else:
                    j -= 1
            if j >= 0:
                cut = j - 1
                while cut >= 0 and s[cut] in " \t\n\r,":
                    cut -= 1
        head = s[: cut + 1]
    else:
        head = s

    head = head.rstrip().rstrip(",:").rstrip()
    head += "".join(reversed(stack))
    return head


def _parse_blocks_lenient(raw: str) -> list[Any]:
    """Parse OCR output extra-defensively: strict parser first, then
    JSON repair on truncation, then a last-ditch raw-text fallback so
    the pipeline still gets *some* content for the page."""
    try:
        return _parse_json_array(raw)
    except RuntimeError:
        pass

    cleaned = _strip_code_fences(raw)
    repaired = _repair_truncated_json(cleaned)
    if repaired is not None:
        parsed = _try_load(repaired)
        if parsed is not None:
            coerced = _coerce_to_block_list(parsed)
            if coerced is not None:
                log("recovered from truncated OCR JSON via repair")
                return coerced

    fallback = cleaned.strip()
    if fallback:
        log("OCR JSON unrecoverable; emitting raw-text fallback block")
        return _split_recognized_text(fallback)
    raise RuntimeError(f"model did not return parseable JSON: {raw[:200]}")


# ── Structured page output (Block schema) ──────────────────────────
#
# The generator model emits a small, typed DSL — the Rust side renders
# blocks → HTML using vetted templates (see ``pipeline::render``). Free-
# form HTML output was the source of the page-render drift we kept
# patching (clipped text, black SVG voids, missing theme CSS). With this
# DSL the model only writes content; the host owns layout + style.

_BLOCKS_PROMPT = """/no_think
You are a learning-page author for the Smilie app. Output ONE JSON object \
matching the schema below — and NOTHING ELSE. No prose, no thinking, no \
markdown fences. Start your response with `{{`.

SCHEMA:
{{
  "title": "<page title>",
  "blocks": [ <Block>, ... ]
}}

Each Block has a `type` discriminator. Allowed Block shapes:
  {{ "type": "heading", "level": 2|3, "text": "..." }}
  {{ "type": "paragraph", "text": "..." }}
  {{ "type": "callout", "kind": "info"|"warn"|"key"|"note", "title": "...", "body": "..." }}
  {{ "type": "key_term", "term": "...", "definition": "..." }}
  {{ "type": "formula", "latex": "...", "caption": "..." }}
  {{ "type": "list", "items": ["...", "..."], "ordered": false }}
  {{ "type": "quiz", "questions": [{{ "q": "...", "choices": ["a","b","c"], "answer": 0, \
"explanation": "..." }}] }}
  {{ "type": "diagram_flow", "title": "...", "nodes": [{{ "id": "a", "label": "...", \
"note": "..." }}], "edges": [{{ "from": "a", "to": "b", "label": "..." }}] }}
  {{ "type": "source_ref", "section_ids": ["s1"], "page_numbers": [3] }}
  {{ "type": "code", "lang": "python", "code": "..." }}

RULES:
  • 4–10 blocks; lead with a heading and a clear paragraph summary.
  • For type=`concept` pages prefer `key_term` + at least one `diagram_flow`.
  • For type=`formula` pages include a `formula` block.
  • For type=`quiz` pages include exactly one `quiz` block (3–5 questions).
  • End with a `source_ref` citing the spec's `source_sections` + `source_pages`.
  • Plain text only inside fields — no HTML tags, no markdown.
  • Do NOT emit any block types not listed above.
  • ASCII for math symbols inside `paragraph.text`; LaTeX only inside `formula.latex`.

PAGE SPEC:
{spec}
"""


# Reading-level prefixes prepended to `_BLOCKS_PROMPT` so the same page
# spec re-renders at the requested audience without changing the block
# schema. Mirrors the `ReadingLevel` enum in `src/lib/ipc/index.ts`.
_LEVEL_PROMPT_PREFIX = {
    "eli5": (
        "AUDIENCE: a smart 12-year-old. Use everyday analogies. Skip jargon. "
        "Short sentences. Prefer concrete examples over formal definitions.\n"
    ),
    "expert": (
        "AUDIENCE: a domain expert. Skip motivation and basic definitions. "
        "Use field-standard terminology. Be terse and precise.\n"
    ),
    # The "default" level matches the existing prompt — no prefix. We
    # explicitly leave it unset so an unrecognised value (e.g. None)
    # also lands on the default audience.
    "default": "",
}


# Two-paper diff prompt. Output shape mirrors `_BLOCKS_PROMPT` (the
# learning-page block schema) so the existing Rust renderer handles the
# diff page identically to a learning page.
_COMPARE_PROMPT = """/no_think
You are a research tutor diffing two scientific papers. Output ONE JSON \
object matching the PageBlocks schema and NOTHING else. No prose, no \
thinking, no markdown fences. Start your response with `{{`.

PageBlocks: {{ "title": "<comparison title>", "blocks": [<Block>, ...] }}
Block shapes are the same as a learning page (heading, paragraph, \
callout, key_term, formula, list, quiz, diagram_flow, source_ref, code).

REQUIRED SECTIONS (in order):
  1. heading — comparison title.
  2. paragraph — one-line shared topic summary.
  3. heading "Shared concepts" + paragraph/list.
  4. heading "Contradictions" + paragraph/list (use callout type=warn \
when a direct contradiction appears).
  5. heading "Unique to paper A" + list of bullets.
  6. heading "Unique to paper B" + list of bullets.
  7. OPTIONAL diagram_flow — only when both papers describe a similar \
pipeline; nodes label the overlapping steps.
  8. source_ref — final block.

Plain text only inside fields. ASCII math symbols inside paragraphs.

PAPER A:
{a}

PAPER B:
{b}
"""


_BLOCK_TYPE_ALIASES = {
    "h": "heading",
    "h2": "heading",
    "h3": "heading",
    "subheading": "heading",
    "p": "paragraph",
    "text": "paragraph",
    "summary": "paragraph",
    "definition": "key_term",
    "term": "key_term",
    "equation": "formula",
    "math": "formula",
    "ul": "list",
    "ol": "list",
    "bullets": "list",
    "diagram": "diagram_flow",
    "flow": "diagram_flow",
    "source": "source_ref",
    "citation": "source_ref",
    "warning": "callout",
    "note": "callout",
    "tip": "callout",
    "info": "callout",
    "key": "callout",
}

_ALLOWED_BLOCK_TYPES = {
    "heading", "paragraph", "callout", "key_term", "formula",
    "list", "quiz", "diagram_flow", "source_ref", "code",
}


def _normalize_block(b: Any) -> dict[str, Any] | None:
    """Coerce one model-emitted block dict to the Rust ``Block`` shape.

    Drops blocks we can't repair so the renderer never panics on a
    surprise variant. Aliases common type synonyms (`equation` →
    `formula`, etc.) so different prompts converge on the same shape.
    """
    if isinstance(b, str) and b.strip():
        return {"type": "paragraph", "text": b.strip()}
    if not isinstance(b, dict):
        return None

    raw_type = (b.get("type") or b.get("kind") or b.get("block_type") or "").strip().lower()
    btype = _BLOCK_TYPE_ALIASES.get(raw_type, raw_type)
    if btype not in _ALLOWED_BLOCK_TYPES:
        # Last-ditch: treat as paragraph if there's any text.
        text = _as_string(b.get("text") or b.get("body") or b.get("content"))
        if text:
            return {"type": "paragraph", "text": text}
        return None

    if btype == "heading":
        level = b.get("level")
        try:
            level = int(level) if level is not None else 2
        except (TypeError, ValueError):
            level = 2
        return {
            "type": "heading",
            "level": max(2, min(level, 4)),
            "text": _as_string(b.get("text") or b.get("title")) or "Untitled",
        }

    if btype == "paragraph":
        return {"type": "paragraph", "text": _as_string(b.get("text") or b.get("body"))}

    if btype == "callout":
        kind = (b.get("kind") or b.get("variant") or "info").lower()
        if kind not in {"info", "warn", "key", "note"}:
            kind = "info"
        return {
            "type": "callout",
            "kind": kind,
            "title": _as_string(b.get("title")) or None,
            "body": _as_string(b.get("body") or b.get("text") or b.get("content")),
        }

    if btype == "key_term":
        return {
            "type": "key_term",
            "term": _as_string(b.get("term") or b.get("title")),
            "definition": _as_string(b.get("definition") or b.get("body") or b.get("text")),
        }

    if btype == "formula":
        return {
            "type": "formula",
            "latex": _clean_latex(b.get("latex") or b.get("equation") or b.get("text")),
            "caption": _as_string(b.get("caption") or b.get("note")) or None,
        }

    if btype == "list":
        items = b.get("items")
        if not isinstance(items, list):
            items = _as_string_list(items)
        items = [_as_string(x) for x in items if _as_string(x)]
        return {
            "type": "list",
            "items": items,
            "ordered": bool(b.get("ordered")),
        }

    if btype == "quiz":
        questions_raw = b.get("questions") or []
        if not isinstance(questions_raw, list):
            questions_raw = []
        questions = []
        for q in questions_raw:
            if not isinstance(q, dict):
                continue
            choices = q.get("choices")
            if not isinstance(choices, list):
                continue
            choices = [_as_string(c) for c in choices if _as_string(c)]
            if len(choices) < 2:
                continue
            try:
                answer = int(q.get("answer", 0))
            except (TypeError, ValueError):
                answer = 0
            answer = max(0, min(answer, len(choices) - 1))
            questions.append({
                "q": _as_string(q.get("q") or q.get("question")),
                "choices": choices,
                "answer": answer,
                "explanation": _as_string(q.get("explanation")) or None,
            })
        return {"type": "quiz", "questions": questions}

    if btype == "diagram_flow":
        nodes_raw = b.get("nodes") or []
        edges_raw = b.get("edges") or []
        nodes: list[dict[str, Any]] = []
        for n in nodes_raw if isinstance(nodes_raw, list) else []:
            if not isinstance(n, dict):
                continue
            nid = _as_string(n.get("id") or n.get("name"))
            label = _as_string(n.get("label") or n.get("title") or n.get("text") or nid)
            if not nid or not label:
                continue
            nodes.append({
                "id": nid,
                "label": label,
                "note": _as_string(n.get("note") or n.get("subtitle")) or None,
            })
        edges: list[dict[str, Any]] = []
        for e in edges_raw if isinstance(edges_raw, list) else []:
            if not isinstance(e, dict):
                continue
            f, t = _as_string(e.get("from")), _as_string(e.get("to"))
            if not f or not t:
                continue
            edges.append({
                "from": f,
                "to": t,
                "label": _as_string(e.get("label")) or None,
            })
        return {
            "type": "diagram_flow",
            "title": _as_string(b.get("title")) or None,
            "nodes": nodes,
            "edges": edges,
        }

    if btype == "source_ref":
        return {
            "type": "source_ref",
            "section_ids": _as_string_list(b.get("section_ids") or b.get("sections")),
            "page_numbers": _as_int_list(b.get("page_numbers") or b.get("pages")),
        }

    if btype == "code":
        return {
            "type": "code",
            "lang": _as_string(b.get("lang") or b.get("language")) or None,
            "code": _as_string(b.get("code") or b.get("text")),
        }

    return None


def _normalize_page_blocks(obj: Any, fallback_title: str) -> dict[str, Any]:
    """Coerce the model's whole-object output into ``PageBlocks`` shape.

    Falls back to a single-paragraph page when nothing parseable is
    inside, so the pipeline always produces SOMETHING viewable instead
    of failing the whole pipeline on a stubborn model.
    """
    if not isinstance(obj, dict):
        obj = {}

    title = _as_string(obj.get("title")) or _as_string(fallback_title) or "Smilie Page"

    raw_blocks = obj.get("blocks")
    if not isinstance(raw_blocks, list):
        # Some models flatten to {"blocks": {"0": {...}}} or skip the
        # wrapper entirely.
        coerced = _coerce_to_block_list(obj)
        raw_blocks = coerced if coerced is not None else []

    blocks: list[dict[str, Any]] = []
    for b in raw_blocks:
        norm = _normalize_block(b)
        if norm is not None:
            blocks.append(norm)

    if not blocks:
        # Last-ditch: synthesize a single paragraph from whatever string
        # data is in the object so the page renders SOMETHING.
        text = _as_string(obj.get("summary") or obj.get("body") or obj.get("text"))
        if text:
            blocks.append({"type": "paragraph", "text": text})

    return {"title": title, "blocks": blocks}


def _parse_json_object(raw: str) -> dict[str, Any]:
    """Parse a JSON object out of the model's free-form output.

    Mirrors `_parse_json_array`'s tolerance: strips ``<think>…</think>``
    Qwen3 prelude, ```json fences, and trailing chat tokens before
    falling back to a brace-slice search.
    """
    cleaned = _strip_code_fences(raw)

    parsed = _try_load(cleaned)
    if isinstance(parsed, dict):
        return parsed

    o_start = cleaned.find("{")
    o_end = cleaned.rfind("}")
    if o_start >= 0 and o_end > o_start:
        parsed = _try_load(cleaned[o_start : o_end + 1])
        if isinstance(parsed, dict):
            return parsed

    raise RuntimeError(f"model did not return JSON object: {raw[:200]}")


# ── Experiment prompts + helpers ───────────────────────────────────
#
# Constraints keep the model on a small allowed package set (numpy +
# scipy + stdlib) so both the host-side python3 test loop AND the
# frontend Pyodide runner can execute the same code without diverging.

_EXPERIMENT_GENERATE_PROMPT = """/no_think
You are writing a runnable Python experiment for the Smilie app. Output ONE \
JSON object and NOTHING else. No prose, no thinking, no code fences. Start \
your response with `{{`.

SHAPE:
{{
  "python_code":  "<self-contained module body>",
  "python_tests": "<def test_*(): functions with assert bodies>",
  "inputs_schema":  [ <InputSpec>, ... ],
  "outputs_schema": [ <OutputSpec>, ... ]
}}

CONSTRAINTS on python_code:
  - Allowed imports: numpy, scipy, math, statistics, random, json. \
micropip is fine when needed.
  - Read variables from a global `INPUTS: dict`, write results into a \
global `OUTPUTS: dict`. Do NOT call sys.exit, do NOT read files.
  - <= 200 lines total. No `if __name__ == "__main__":` guard needed.

CONSTRAINTS on python_tests (HARD RULES — runner enforces):
  - Tests MUST be `def test_<name>():` functions. The runner discovers \
each function whose name starts with `test_` and calls it in isolation, \
recording per-function pass/fail.
  - Each test function body uses plain `assert` statements. \
Do NOT `import pytest`. Do NOT use top-level / module-scope asserts — \
those will be REJECTED.
  - Tests run AFTER `python_code`; functions reference the global \
`OUTPUTS` (and `INPUTS` if needed) directly.

EXAMPLE python_tests (correct shape):
```
def test_outputs_exists():
    assert "x" in OUTPUTS

def test_x_is_positive():
    assert OUTPUTS["x"] > 0
```

InputSpec ::=
  {{name, kind:"number"|"int", default, label, min?, max?, step?, help?}}
  | {{name, kind:"choice", default, options:[...], label, help?}}
  | {{name, kind:"text",   default, label, help?}}
  | {{name, kind:"vector", default:[...], length, label, help?}}

OutputSpec ::=
  {{name, kind:"number", label, precision?}}
  | {{name, kind:"text",   label}}
  | {{name, kind:"table",  label, columns:[...]}}
  | {{name, kind:"series", label, x_label?, y_label?}}  # value: {{x:[],y:[]}}

APPROACH:
{approach}

PAPER JSON:
{paper}
"""


_EXPERIMENT_REFINE_PROMPT = """/no_think
You are repairing a previous Python experiment for the Smilie app. Output \
ONE JSON object and NOTHING else with the SAME shape as before: \
{{"python_code": "...", "python_tests": "...", "inputs_schema": [...], \
"outputs_schema": [...]}}.

REMINDER: `python_tests` MUST contain `def test_<name>():` functions \
with `assert` bodies — NEVER bare top-level asserts. Example:
```
def test_outputs_exists():
    assert "x" in OUTPUTS
```

{directive}

PRIOR CODE:
{code}

PRIOR TESTS:
{tests}

ERROR / OUTPUT:
{error_output}

PAPER JSON:
{paper}
"""


# One-shot retry prompt used when the model returned tests that lack any
# `def test_` wrapper. We keep it brief and focused — the model already
# saw the full instructions, this is just a corrective nudge.
_EXPERIMENT_RETRY_TESTS_PROMPT = """/no_think
Your previous response had `python_tests` written as bare top-level \
`assert` statements. The Smilie runner REQUIRES `def test_<name>():` \
functions instead. Re-emit the SAME JSON object, but rewrite \
`python_tests` so every assertion lives inside a function whose name \
starts with `test_`. Keep `python_code`, `inputs_schema`, and \
`outputs_schema` identical.

EXAMPLE of the required test shape:
```
def test_outputs_exists():
    assert "x" in OUTPUTS

def test_x_positive():
    assert OUTPUTS["x"] > 0
```

Output ONE JSON object and nothing else. No prose, no fences.

PRIOR python_code (keep verbatim):
{code}

PRIOR python_tests (rewrite into functions):
{tests}
"""


# Regex used by the host-side validator and tests. Matches `def test_<name>(`
# at any indentation — handles both common LLM output styles. We use a
# raw bytes-level pattern instead of `ast.parse` because some valid
# emissions still trip the parser (mixed indentation, stray fence
# remnants) and we'd rather accept a slightly malformed-but-functional
# emission than reject it.
_TEST_FN_RE = re.compile(r"(?m)^\s*def\s+test_[A-Za-z0-9_]*\s*\(")


def _has_test_functions(tests_src: str) -> bool:
    """True iff `tests_src` contains at least one `def test_*(` definition.

    Used by the experiment generate/refine endpoints to gate a one-shot
    retry when the model emits bare top-level asserts. The runner side
    (`experiment-worker.js`) discovers tests via the same convention,
    so this is the contract we enforce here.
    """
    if not isinstance(tests_src, str) or not tests_src.strip():
        return False
    return _TEST_FN_RE.search(tests_src) is not None


def _slim_paper_for_analyze(paper: Any) -> str:
    """Compact a PaperJson down to what a reproducibility verdict
    actually needs: title + authors + abstract + per-section heading +
    first paragraph. Skips equations, figure captions, references.

    Roughly 5–10k chars vs the previous 120k truncation → cuts the
    reasoning model's prefill by ~10× on Apple Silicon. The plausibility
    call doesn't need full method bodies; the abstract + heading map
    captures the central claim well enough to score it.
    """
    if not isinstance(paper, dict):
        return json.dumps(paper)[:8_000]

    parts: list[str] = []

    title = _as_string(paper.get("title"))
    if title:
        parts.append(f"TITLE: {title}")

    authors = paper.get("authors")
    if isinstance(authors, list) and authors:
        joined = ", ".join(_as_string(a) for a in authors[:8] if _as_string(a))
        if joined:
            parts.append(f"AUTHORS: {joined}")

    sections = paper.get("sections")
    if isinstance(sections, list):
        for i, section in enumerate(sections):
            if not isinstance(section, dict):
                continue
            heading = _as_string(section.get("title"))
            text = _as_string(section.get("text"))
            if not heading and not text:
                continue
            # First section (usually abstract or intro) gets up to
            # 1500 chars; subsequent sections get heading + 350 chars
            # of body — enough for the verdict, not enough to blow
            # prefill.
            limit = 1500 if i == 0 else 350
            snippet = text[:limit].rstrip()
            label = heading or f"Section {i + 1}"
            parts.append(f"\n## {label}\n{snippet}".rstrip())

    # Hard cap as a safety net — typical papers will land ~3-7k chars.
    return "\n".join(parts)[:10_000]


def _parse_experiment_json(raw: str) -> dict[str, Any]:
    """Pull a JSON object out of the model's free-form output.

    Mirrors the reasoning path: try the strict parser first, then
    repair-truncated, then a brace-slice fallback. Always returns a
    dict — raises only if every strategy fails.
    """
    try:
        return _parse_json_object(raw)
    except RuntimeError:
        pass
    cleaned = _strip_code_fences(raw)
    repaired = _repair_truncated_json(cleaned)
    if repaired:
        parsed = _try_load(repaired)
        if isinstance(parsed, dict):
            return parsed
    raise RuntimeError(f"experiment model did not return JSON object: {raw[:200]}")


def _coerce_score(v: Any) -> int:
    """Score must be 0..=100. Take whatever the model returned and
    clamp; default to 0 on garbage so the pipeline marks the
    experiment implausible (safe default)."""
    try:
        n = int(float(v))
    except (TypeError, ValueError):
        return 0
    return max(0, min(100, n))


def _normalize_experiment_analysis(obj: Any) -> dict[str, Any]:
    if not isinstance(obj, dict):
        obj = {}
    score = _coerce_score(obj.get("score", 0))
    plausible = obj.get("plausible")
    if plausible is None:
        plausible = score >= 50
    return {
        "plausible": bool(plausible),
        "score": score,
        "reasoning": _as_string(obj.get("reasoning")),
        "approach_summary": _as_string(obj.get("approach_summary")),
        "suggested_title": _as_string(obj.get("suggested_title")),
    }


def _normalize_experiment_artifacts(obj: Any) -> dict[str, Any]:
    if not isinstance(obj, dict):
        obj = {}
    inputs = obj.get("inputs_schema")
    if not isinstance(inputs, list):
        inputs = []
    outputs = obj.get("outputs_schema")
    if not isinstance(outputs, list):
        outputs = []
    return {
        "python_code": _as_string(obj.get("python_code")),
        "python_tests": _as_string(obj.get("python_tests")),
        "inputs_schema": inputs,
        "outputs_schema": outputs,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-path", required=True)
    args = parser.parse_args()

    sidecar = Sidecar(Path(args.model_path))
    log(f"ready; model_path={args.model_path}")

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            _respond({"id": "", "ok": False, "error": f"bad json: {e}"})
            continue

        req_id = req.get("id", "")
        method = req.get("method", "")
        params = req.get("params", {})

        try:
            if method == "ocr":
                result = sidecar.ocr(params)
            elif method == "reason":
                result = sidecar.reason(params)
            elif method == "generate":
                result = sidecar.generate(params)
            elif method == "analyze_experiment":
                result = sidecar.analyze_experiment(params)
            elif method == "generate_experiment":
                result = sidecar.generate_experiment(params)
            elif method == "refine_experiment":
                result = sidecar.refine_experiment(params)
            elif method == "compare_papers":
                result = sidecar.compare_papers(params)
            else:
                raise RuntimeError(f"unknown method: {method}")
            _respond({"id": req_id, "ok": True, "result": result})
        except _MissingWeights as e:
            _respond(
                {
                    "id": req_id,
                    "ok": False,
                    "error": f"missing_weights:{e.path}",
                }
            )
        except Exception as e:
            log("error: " + traceback.format_exc())
            _respond({"id": req_id, "ok": False, "error": str(e)})


def _respond(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
