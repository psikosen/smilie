# `_USER_CODE` and `_USER_TESTS` are PREPENDED to this file as string
# literals by `test_loop.rs::build_driver` before `python3` is
# invoked. They are not defined when this file is read standalone —
# Pyright will flag them as undefined; that's expected. We use
# `# type: ignore` on the references rather than declaring defaults,
# because any default assignment placed AFTER the prepend (i.e. in
# this file) would clobber the real values at runtime.

import sys
import traceback

INPUTS = {}
OUTPUTS = {}

# ---- user code ----
try:
    exec(_USER_CODE, globals())  # type: ignore[name-defined]
except Exception:
    print("MODULE_IMPORT_ERROR", file=sys.stderr)
    traceback.print_exc()
    sys.exit(2)

# ---- user tests (definitions only) ----
try:
    exec(_USER_TESTS, globals())  # type: ignore[name-defined]
except Exception:
    print("TESTS_IMPORT_ERROR", file=sys.stderr)
    print(
        "tests source must define `def test_*():` functions. "
        "Bare top-level `assert` is rejected.",
        file=sys.stderr,
    )
    traceback.print_exc()
    sys.exit(3)

names = [n for n in dir() if n.startswith("test_") and callable(globals().get(n))]
if not names:
    print("NO_TESTS_FOUND", file=sys.stderr)
    print(
        "tests source did not define any `def test_*():` functions.",
        file=sys.stderr,
    )
    sys.exit(4)

results = []
any_fail = False
for n in sorted(names):
    fn = globals()[n]
    try:
        fn()
        results.append(("PASS", n, ""))
    except AssertionError as e:
        results.append(("FAIL", n, str(e) or "AssertionError"))
        any_fail = True
    except Exception as e:
        results.append(("ERROR", n, f"{type(e).__name__}: {e}"))
        any_fail = True

for s, name, msg in results:
    if msg:
        print(f"{s} {name}: {msg}")
    else:
        print(f"{s} {name}")

sys.exit(1 if any_fail else 0)
