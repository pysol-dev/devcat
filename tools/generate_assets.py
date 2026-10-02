#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.13"
# dependencies = ["pillow==11.3.0", "fonttools==4.59.2"]
# ///
# ─── How to run ───
# 1. Install uv: curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run: uv run tools/generate_assets.py
# 3. Or: chmod +x tools/generate_assets.py && ./tools/generate_assets.py
# ──────────────────
"""Derive BongoCat frames from the pinned upstream font without redrawing art."""

from __future__ import annotations

import hashlib
import json
from io import BytesIO
from pathlib import Path
from typing import Final, TypedDict

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont, features

ROOT: Final = Path(__file__).resolve().parents[1]
SOURCE: Final = ROOT / "assets/source"
OUTPUT: Final = ROOT / "assets/generated"
FONT_SHA256: Final = "5b9bc74991b8abdb04785e4ef16db86bef867d5b19fa60444dbca11230224225"
POSES: Final = {"idle": "bc", "left": "dc", "right": "ba"}
CANVAS: Final = (272, 152)
ORIGIN: Final = (8, 128)
EM_SIZE: Final = 128
CELL_PIXELS: Final = (34, 20)
COMPACT_PIXELS: Final = (16, 10)
MONO_THRESHOLD: Final = 96


class SourceMismatchError(RuntimeError):
    """The source font no longer matches the audited upstream artifact."""

    def __init__(self, actual: str) -> None:
        super().__init__(f"BongoCat font SHA-256 mismatch: {actual}")


class CellFrame(TypedDict):
    """Top-to-bottom alpha samples and thresholded half-block rows."""

    alpha_rows: list[str]
    monochrome_rows: list[str]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_font() -> ImageFont.FreeTypeFont:
    """Decode WOFF in memory; use the original outlines and advance widths."""
    actual = sha256(SOURCE / "bongocat.woff")
    if actual != FONT_SHA256:
        raise SourceMismatchError(actual)
    with TTFont(SOURCE / "bongocat.woff", recalcTimestamp=False) as source:
        source.flavor = None
        with BytesIO() as buffer:
            source.save(buffer)
            return ImageFont.truetype(
                BytesIO(buffer.getvalue()), EM_SIZE,
                layout_engine=ImageFont.Layout.BASIC,
            )


def render_frame(pair: str, font: ImageFont.FreeTypeFont) -> Image.Image:
    """Place two icon glyphs in adjacent em boxes, as VS Code does."""
    frame = Image.new("RGBA", CANVAS)
    for index, glyph in enumerate(pair):
        layer = Image.new("RGBA", CANVAS)
        ImageDraw.Draw(layer).text(
            (ORIGIN[0] + index * EM_SIZE, ORIGIN[1]), glyph,
            font=font, fill=(255, 255, 255, 255), anchor="ls",
        )
        frame.alpha_composite(layer)
    return frame


def cell_frame(frame: Image.Image, pixels: tuple[int, int] = CELL_PIXELS) -> CellFrame:
    """Store theme-independent alpha coverage; each cell has two samples."""
    alpha = frame.getchannel("A").resize(pixels, Image.Resampling.LANCZOS)
    width, height = alpha.size
    samples = alpha.tobytes()
    rows = [samples[y * width:(y + 1) * width] for y in range(height)]
    blocks = " ▀▄█"
    return {
        "alpha_rows": [row.hex() for row in rows],
        "monochrome_rows": [
            "".join(
                blocks[(rows[y][x] >= MONO_THRESHOLD)
                       + 2 * (rows[y + 1][x] >= MONO_THRESHOLD)]
                for x in range(width)
            )
            for y in range(0, height, 2)
        ],
    }


def preview(
    frames: list[Image.Image], cell_pixels: tuple[int, int] = CELL_PIXELS,
) -> Image.Image:
    """Show each source silhouette and cell approximation on both themes."""
    sheet = Image.new("RGB", (864, 680))
    draw = ImageDraw.Draw(sheet)
    for theme_index, (background, foreground) in enumerate(
        (((30, 30, 30), (240, 240, 240)), ((245, 245, 245), (30, 30, 30))),
    ):
        top = theme_index * 340
        draw.rectangle((0, top, 863, top + 339), fill=background)
        for index, (name, frame) in enumerate(zip(POSES, frames, strict=True)):
            left = index * 288 + 8
            draw.text((left + 8, top + 10), name, fill=foreground)
            ink = Image.new("RGB", frame.size, foreground)
            sheet.paste(ink, (left, top + 34), frame.getchannel("A"))
            pixels = frame.getchannel("A").resize(
                cell_pixels, Image.Resampling.LANCZOS,
            ).resize(CANVAS, Image.Resampling.NEAREST)
            sheet.paste(ink, (left, top + 174), pixels)
    return sheet


def main() -> None:
    """Generate frames, cell data, visual proof sheet, and content manifest."""
    font = load_font()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    frames = [render_frame(pair, font) for pair in POSES.values()]
    for name, frame in zip(POSES, frames, strict=True):
        frame.save(OUTPUT / f"{name}.png", compress_level=9)
    for filename, pixels in (("cells.json", CELL_PIXELS),
                             ("compact-cells.json", COMPACT_PIXELS)):
        cells = {
            "schema_version": 1,
            "columns": pixels[0], "rows": pixels[1] // 2,
            "encoding": "alpha8-hex-rows-top-to-bottom",
            "foreground_rgb": [255, 255, 255],
            "monochrome_threshold": MONO_THRESHOLD,
            "frames": {name: cell_frame(frame, pixels)
                       for name, frame in zip(POSES, frames, strict=True)},
        }
        (OUTPUT / filename).write_text(
            json.dumps(cells, ensure_ascii=False, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
    preview(frames).save(OUTPUT / "preview.png", compress_level=9)
    preview(frames, COMPACT_PIXELS).save(OUTPUT / "compact-preview.png", compress_level=9)
    products = [*(f"{name}.png" for name in POSES), "cells.json", "preview.png",
                "compact-cells.json", "compact-preview.png"]
    manifest = {
        "schema_version": 1, "generator_version": 1,
        "generator_sha256": sha256(Path(__file__)),
        "source_repository": "https://github.com/kitgore/BongoCat",
        "source_commit": "20e59c96cd25e4dcc098a5ab908bb27cdd4caabc",
        "license": "MIT", "copyright": "Copyright (c) 2023 ben",
        "inputs": {path.name: sha256(path) for path in sorted(SOURCE.iterdir())
                   if path.is_file()},
        "toolchain": {"pillow": "11.3.0", "fonttools": "4.59.2",
                      "freetype": features.version_module("freetype2")},
        "glyph_pairs": POSES, "canvas": list(CANVAS),
        "glyph_em_pixels": EM_SIZE, "origin_baseline": list(ORIGIN),
        "glyph_advance_pixels": EM_SIZE,
        "palette_rgba": [255, 255, 255, 255],
        "cell_pixels": list(CELL_PIXELS),
        "compact_cell_pixels": list(COMPACT_PIXELS),
        "outputs": {name: sha256(OUTPUT / name) for name in products},
    }
    (OUTPUT / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8",
    )
    print(f"Generated {len(frames)} poses at {CANVAS}; manifest: {OUTPUT / 'manifest.json'}")


if __name__ == "__main__":
    main()
