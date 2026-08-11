"""Generate placeholder app icons in the Night-Tokyo palette.

Run once: ``python3 scripts/make_placeholder_icons.py``. Outputs into
``src-tauri/icons/``. Replace with real artwork before shipping.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw


OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

BG = (7, 6, 11, 255)          # --bg-void
BG_EDGE = (21, 16, 31, 255)   # --bg-elevated
VIOLET = (106, 45, 255, 255)  # --violet-500
VIOLET_HI = (181, 128, 255, 255)
CYAN = (79, 240, 255, 255)


def make_master(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), BG)
    draw = ImageDraw.Draw(img)

    # Radial-ish background via concentric rounded rects
    for i in range(4):
        pad = i * size // 24
        draw.rounded_rectangle(
            (pad, pad, size - pad, size - pad),
            radius=size // 6,
            fill=(BG_EDGE[0] + i * 3, BG_EDGE[1] + i * 3, BG_EDGE[2] + i * 4, 255),
        )

    # Purple diamond mark (refs the "S" in Smilie)
    cx, cy = size // 2, size // 2
    r = int(size * 0.30)
    points = [(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)]
    draw.polygon(points, fill=VIOLET)
    draw.polygon(
        [
            (cx, cy - int(r * 0.8)),
            (cx + int(r * 0.8), cy),
            (cx, cy + int(r * 0.8)),
            (cx - int(r * 0.8), cy),
        ],
        outline=VIOLET_HI,
        width=max(1, size // 128),
    )

    # Cyan accent dot
    dot = max(2, size // 32)
    draw.ellipse(
        (cx + int(r * 0.45) - dot, cy - int(r * 0.45) - dot,
         cx + int(r * 0.45) + dot, cy - int(r * 0.45) + dot),
        fill=CYAN,
    )

    return img


def main() -> None:
    master = make_master(1024)

    sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }
    for name, s in sizes.items():
        master.resize((s, s), Image.Resampling.LANCZOS).save(OUT / name)

    # Windows .ico (multi-res)
    ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    master.save(OUT / "icon.ico", sizes=ico_sizes)

    # macOS .icns — Pillow supports it via multi-frame image
    icns_sizes = [(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)]
    icns_frames = [master.resize(s, Image.Resampling.LANCZOS) for s in icns_sizes]
    icns_frames[0].save(
        OUT / "icon.icns",
        format="ICNS",
        append_images=icns_frames[1:],
    )

    print(f"wrote icons to {OUT}")


if __name__ == "__main__":
    main()
