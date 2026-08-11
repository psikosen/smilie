"""Unit tests for the OCR parser quirks.

Covers every weird shape we have observed the model produce — so when
we see a new one, we add a case here and the parser stops being
surprised.

Pyright import: the module is found at runtime via `python/` on
sys.path (set by conftest or by `pytest -s` from `src-tauri/python/`).
Static analysis in this repo doesn't have that path, so we silence the
import-resolution warning at the source.
"""

import pytest

from mlx_sidecar.server import (  # type: ignore[reportMissingImports]
    _coerce_to_block_list,
    _clean_latex,
    _has_test_functions,
    _normalize_experiment_artifacts,
    _normalize_ocr_block,
    _normalize_page_spec,
    _normalize_plan,
    _parse_blocks_lenient,
    _parse_experiment_json,
    _parse_json_array,
    _parse_json_object,
    _repair_truncated_json,
    _split_recognized_text,
    _strip_code_fences,
)


# ── _strip_code_fences ──────────────────────────────────────────

def test_strip_strips_json_fence():
    assert _strip_code_fences("```json\n[1,2,3]\n```") == "[1,2,3]"


def test_strip_strips_bare_fence():
    assert _strip_code_fences("```\n{}\n```") == "{}"


def test_strip_strips_trailing_chat_token():
    assert _strip_code_fences("[]<|user|>") == "[]"


def test_strip_noop_when_no_fence():
    assert _strip_code_fences("[1,2,3]") == "[1,2,3]"


# ── _coerce_to_block_list ───────────────────────────────────────

def test_coerce_list_passes_through():
    assert _coerce_to_block_list([1, 2]) == [1, 2]


def test_coerce_unwraps_blocks_key():
    assert _coerce_to_block_list({"blocks": [{"text": "hi"}]}) == [{"text": "hi"}]


def test_coerce_wraps_single_block():
    obj = {"block_type": "header", "text": "Title"}
    assert _coerce_to_block_list(obj) == [obj]


def test_coerce_dict_of_blocks():
    obj = {"0": {"text": "a"}, "1": {"text": "b"}}
    out = _coerce_to_block_list(obj)
    assert out == [{"text": "a"}, {"text": "b"}]


def test_coerce_returns_none_for_garbage():
    assert _coerce_to_block_list(42) is None
    assert _coerce_to_block_list("hello") is None


# ── _parse_json_array ───────────────────────────────────────────

def test_parses_clean_array():
    out = _parse_json_array('[{"text":"a"},{"text":"b"}]')
    assert len(out) == 2


def test_parses_fenced_array():
    out = _parse_json_array('```json\n[{"text":"a"}]\n```')
    assert out == [{"text": "a"}]


def test_parses_single_block_dict():
    """The exact failure mode user reported: model returns one fenced dict."""
    raw = (
        '```json\n'
        '{ "block_type": "header", "text": "Efficient Universal Perception" }\n'
        '```'
    )
    out = _parse_json_array(raw)
    assert len(out) == 1
    assert out[0]["block_type"] == "header"


def test_parses_blocks_key_dict():
    raw = '{"blocks":[{"text":"a"}]}'
    assert _parse_json_array(raw) == [{"text": "a"}]


def test_parses_with_leading_prose():
    raw = "Here are the blocks:\n[{\"text\":\"a\"}]"
    assert _parse_json_array(raw) == [{"text": "a"}]


def test_parses_unfenced_single_dict_with_inner_bbox():
    """Inner `[1,2,3,4]` in a bbox must not be mistaken for the outer array."""
    raw = '{"block_type":"text","text":"hi","bbox":[10,20,30,40]}'
    out = _parse_json_array(raw)
    assert len(out) == 1
    assert out[0]["bbox"] == [10, 20, 30, 40]


def test_raises_on_total_garbage():
    with pytest.raises(RuntimeError):
        _parse_json_array("the model said nothing useful")


# ── _normalize_ocr_block ────────────────────────────────────────

def test_normalize_string_to_text_block():
    out = _normalize_ocr_block("hello", page_number=3)
    assert out == {"page_number": 3, "block_type": "text", "text": "hello"}


def test_normalize_invalid_block_type_falls_back_to_text():
    out = _normalize_ocr_block({"block_type": "weird_type", "text": "x"}, 1)
    assert out is not None
    assert out["block_type"] == "text"


def test_normalize_uses_type_alias():
    out = _normalize_ocr_block({"type": "heading", "text": "x"}, 1)
    assert out is not None
    assert out["block_type"] == "heading"


def test_normalize_equation_alias_and_latex_field():
    out = _normalize_ocr_block(
        {"type": "formula", "latex": r"\[ E = mc^2 \]"}, 2
    )
    assert out == {
        "page_number": 2,
        "block_type": "equation",
        "text": "E = mc^2",
    }


@pytest.mark.parametrize(
    ("source", "expected"),
    [
        (r"$$x_1 + x_2$$", "x_1 + x_2"),
        (r"\(\frac{a}{b}\)", r"\frac{a}{b}"),
        ("```latex\n\\sum_i x_i\n```", r"\sum_i x_i"),
    ],
)
def test_clean_latex_removes_transport_wrappers(source, expected):
    assert _clean_latex(source) == expected


def test_split_recognized_text_promotes_display_math():
    out = _split_recognized_text(
        "Before.\n\n$$E = mc^2$$\n\nMiddle.\n\\[x_i = \\frac{a}{b}\\]\nAfter."
    )
    assert [block["block_type"] for block in out] == [
        "text", "equation", "text", "equation", "text"
    ]
    assert out[1]["text"] == "E = mc^2"
    assert out[3]["text"] == r"x_i = \frac{a}{b}"


def test_normalize_list_text_joined():
    out = _normalize_ocr_block({"text": ["line 1", "line 2"]}, 1)
    assert out is not None
    assert out["text"] == "line 1\nline 2"


def test_normalize_drops_non_dict_non_str():
    assert _normalize_ocr_block(42, 1) is None
    assert _normalize_ocr_block(None, 1) is None


def test_normalize_keeps_valid_bbox_and_confidence():
    out = _normalize_ocr_block(
        {"text": "x", "bbox": [1, 2, 3, 4], "confidence": 0.9}, 1
    )
    assert out is not None
    assert out["bbox"] == [1.0, 2.0, 3.0, 4.0]
    assert out["confidence"] == 0.9


# ── _strip_code_fences with <think> ─────────────────────────────

def test_strip_drops_think_block():
    raw = "<think>I will plan…</think>\n```json\n[]\n```"
    assert _strip_code_fences(raw) == "[]"


def test_strip_drops_multiple_think_blocks():
    raw = "<think>a</think>{}<think>b</think>"
    assert _strip_code_fences(raw) == "{}"


# ── _parse_json_object ──────────────────────────────────────────

def test_parse_object_clean():
    assert _parse_json_object('{"summary":"hi","pages":[]}') == {
        "summary": "hi", "pages": []
    }


def test_parse_object_with_think_prelude():
    """Qwen3 reasoning model fail mode: <think>…</think> before JSON."""
    raw = (
        "<think>Let me plan the lessons. There will be 3 pages.</think>\n"
        '{"summary": "ok", "pages": [{"title":"a","type":"informational"}]}'
    )
    out = _parse_json_object(raw)
    assert out["summary"] == "ok"
    assert len(out["pages"]) == 1


def test_parse_object_with_fence():
    raw = '```json\n{"summary":"x","pages":[]}\n```'
    out = _parse_json_object(raw)
    assert out["summary"] == "x"


def test_parse_object_raises_on_garbage():
    with pytest.raises(RuntimeError):
        _parse_json_object("nothing parseable")


# `_extract_html` was removed when the generator switched to
# structured-block output (the entire free-form HTML codepath was
# retired). Five tests covering it lived here; they are deleted along
# with the helper.

# ── _repair_truncated_json ──────────────────────────────────────

def test_repair_closes_open_object():
    out = _repair_truncated_json('{"a": 1, "b": 2')
    assert out == '{"a": 1, "b": 2}'


def test_repair_closes_open_array_of_objects():
    out = _repair_truncated_json('[{"a": 1}, {"b": 2')
    assert out == '[{"a": 1}, {"b": 2}]'


def test_repair_drops_unfinished_string_value():
    """Exact pattern from the user's failure: text field cut mid-content."""
    raw = '{"block_type": "header", "text": "Efficient Universal'
    out = _repair_truncated_json(raw)
    assert out is not None
    import json as _json
    parsed = _json.loads(out)
    assert parsed["block_type"] == "header"


def test_repair_drops_dangling_comma_then_closes():
    out = _repair_truncated_json('[{"a": 1}, {"b": 2}, ')
    assert out == '[{"a": 1}, {"b": 2}]'


def test_repair_returns_none_for_mismatched_brackets():
    assert _repair_truncated_json("} extra }") is None


def test_repair_already_balanced_is_idempotent():
    out = _repair_truncated_json('[{"a": 1}]')
    assert out == '[{"a": 1}]'


# ── _parse_blocks_lenient ──────────────────────────────────────

def test_lenient_handles_strict_success():
    """Happy path: defers to strict parser when input is clean."""
    raw = '[{"block_type":"text","text":"a"}]'
    out = _parse_blocks_lenient(raw)
    assert out == [{"block_type": "text", "text": "a"}]


def test_lenient_repairs_truncated_user_failure():
    """The exact bytes the user pasted in their bug report."""
    raw = (
        '```json\n'
        '{ "block_type": "header", "text": '
        '"Efficient Universal Perception Encoder\\nChenchen Zhu, Saksham Suri,'
    )
    out = _parse_blocks_lenient(raw)
    assert len(out) == 1
    assert out[0]["block_type"] == "header"


def test_lenient_falls_back_to_raw_text_when_unrepairable():
    """Total garbage still gets through as a single text block."""
    raw = "this is not JSON at all but it's text"
    out = _parse_blocks_lenient(raw)
    assert len(out) == 1
    assert out[0]["block_type"] == "text"
    assert "this is not JSON" in out[0]["text"]


# ── _normalize_plan / _normalize_page_spec ─────────────────────

def test_plan_coerces_list_title_to_string():
    """Real failure: model returned `"title": ["x", "y"]` → Rust serde
    croaked with "invalid type: sequence, expected a string"."""
    plan = {
        "summary": "ok",
        "pages": [{
            "title": ["The", "Bigger", "Picture"],
            "type": "informational",
            "source_sections": ["s1"],
            "source_pages": [1],
            "source_content": "snippet",
        }],
    }
    out = _normalize_plan(plan)
    assert isinstance(out["pages"][0]["title"], str)
    assert out["pages"][0]["title"] == "The\n\nBigger\n\nPicture"


def test_plan_coerces_list_source_content_to_string():
    plan = {
        "summary": "x",
        "pages": [{
            "title": "Page 1",
            "type": "concept",
            "source_sections": [],
            "source_pages": [],
            "source_content": ["snippet 1", "snippet 2"],
        }],
    }
    out = _normalize_plan(plan)
    sc = out["pages"][0]["source_content"]
    assert isinstance(sc, str)
    assert "snippet 1" in sc and "snippet 2" in sc


def test_plan_coerces_string_summary_when_model_listed_it():
    out = _normalize_plan({"summary": ["a", "b"], "pages": []})
    assert out["summary"] == "a\n\nb"


def test_plan_invalid_page_type_falls_back():
    out = _normalize_plan({
        "summary": "",
        "pages": [{"title": "x", "type": "garbage_type",
                   "source_content": ""}],
    })
    assert out["pages"][0]["type"] == "informational"


def test_plan_source_pages_string_to_int_list():
    """Tolerate `"source_pages": "1, 2, 3"` and `"[1,2]"`."""
    out = _normalize_plan({
        "summary": "",
        "pages": [{"title": "x", "type": "concept",
                   "source_sections": ["s1"],
                   "source_pages": "1, 2, 3",
                   "source_content": ""}],
    })
    assert out["pages"][0]["source_pages"] == [1, 2, 3]


def test_plan_source_pages_drops_garbage_entries():
    out = _normalize_plan({
        "summary": "",
        "pages": [{"title": "x", "type": "concept",
                   "source_pages": [1, "two", 3, None]}],
    })
    assert out["pages"][0]["source_pages"] == [1, 3]


def test_plan_assets_defaulted_when_missing():
    out = _normalize_plan({"summary": "", "pages": [{"title": "x"}]})
    a = out["pages"][0]["assets"]
    assert a == {"figures": [], "tables": [], "equations": []}


def test_plan_normalize_doesnt_drop_strings_that_already_match():
    """Idempotency: normalizing an already-clean plan must be a no-op
    apart from defaults."""
    page = {
        "title": "ok", "type": "concept",
        "source_sections": ["s1"], "source_pages": [1, 2],
        "source_content": "real text",
        "assets": {"figures": ["f1"], "tables": [], "equations": []},
    }
    out = _normalize_plan({"summary": "s", "pages": [page]})
    assert out["pages"][0]["title"] == "ok"
    assert out["pages"][0]["source_content"] == "real text"
    assert out["pages"][0]["source_pages"] == [1, 2]


def test_normalize_page_spec_with_non_dict_input():
    """If a plan accidentally contains a bare string entry, don't crash."""
    out = _normalize_page_spec("just a title")
    assert out["title"] == "just a title"
    assert out["type"] == "informational"


# ── _has_test_functions ────────────────────────────────────────────
#
# Locks the test-source contract enforced by both the Pyodide worker
# (frontend) and the Rust harness (`src-tauri/src/experiment/test_loop.rs`):
# the model MUST emit `def test_<name>():` functions. Bare top-level
# asserts trigger the one-shot retry inside generate_experiment /
# refine_experiment.

def test_has_test_functions_accepts_def_test():
    src = "def test_thing():\n    assert True\n"
    assert _has_test_functions(src)


def test_has_test_functions_accepts_multiple():
    src = (
        "def test_a():\n    assert 1\n"
        "def test_b():\n    assert 2\n"
    )
    assert _has_test_functions(src)


def test_has_test_functions_accepts_indented_inside_helper_class():
    """Some models nest tests under a class — still counts as wrapped."""
    src = "class X:\n    def test_method(self):\n        assert 1\n"
    assert _has_test_functions(src)


def test_has_test_functions_rejects_bare_top_level_asserts():
    src = "assert OUTPUTS['x'] == 1\nassert OUTPUTS['y'] > 0\n"
    assert not _has_test_functions(src)


def test_has_test_functions_rejects_empty_string():
    assert not _has_test_functions("")
    assert not _has_test_functions("   \n\n")


def test_has_test_functions_rejects_non_string():
    assert not _has_test_functions(None)
    assert not _has_test_functions(42)


def test_has_test_functions_rejects_only_helpers_no_test_prefix():
    """`def helper():` does NOT match — the prefix must be `test_`."""
    src = "def helper():\n    assert True\n"
    assert not _has_test_functions(src)


# ── _parse_experiment_json: tolerance ──────────────────────────────
#
# The experiment endpoints rely on `_parse_experiment_json` to absorb
# the same Qwen3 quirks the rest of the parsers handle. These cases
# replicate the failure modes we have seen in the per-paper pipeline.

def test_parse_experiment_json_strips_think_block():
    raw = (
        "<think>I'll write a small experiment now.</think>\n"
        '{"python_code":"OUTPUTS[\\"x\\"]=1","python_tests":"def test_x():\\n    assert OUTPUTS[\\"x\\"]==1","inputs_schema":[],"outputs_schema":[]}'
    )
    obj = _parse_experiment_json(raw)
    assert "python_code" in obj
    norm = _normalize_experiment_artifacts(obj)
    assert _has_test_functions(norm["python_tests"])


def test_parse_experiment_json_strips_json_fence():
    raw = (
        "```json\n"
        '{"python_code":"","python_tests":"def test_a():\\n    assert True","inputs_schema":[],"outputs_schema":[]}\n'
        "```"
    )
    obj = _parse_experiment_json(raw)
    assert _has_test_functions(obj["python_tests"])


def test_parse_experiment_json_repairs_truncated_object():
    """Model ran out of tokens mid-string. Truncation repair should
    still recover a usable object (with possibly-truncated fields)."""
    raw = (
        '{"python_code":"OUTPUTS[\\"x\\"]=1","python_tests":"def test_a():\\n    assert OUTPUTS[\\"x'
    )
    obj = _parse_experiment_json(raw)
    # We don't assert perfect contents — just that the parser returned
    # *something* dict-shaped instead of raising. The refine loop will
    # re-prompt on the truncated test source.
    assert isinstance(obj, dict)


def test_parse_experiment_json_raises_on_garbage():
    with pytest.raises(RuntimeError):
        _parse_experiment_json("nothing parseable at all")
