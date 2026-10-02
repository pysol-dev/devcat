#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.13"
# dependencies = ["pillow==11.3.0", "fonttools==4.59.2", "pytest==8.4.2"]
# ///
# ─── How to run ───
# 1. Install uv: curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run: uv run tools/test_assets.py
# 3. Or: chmod +x tools/test_assets.py && ./tools/test_assets.py
# ──────────────────
"""Check preserved glyph geometry, cell encoding, and published artifact hashes."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from PIL import Image, ImageChops

from generate_assets import (
    CANVAS, CELL_PIXELS, COMPACT_PIXELS, FONT_SHA256, OUTPUT, SOURCE, cell_frame,
    load_font, render_frame, sha256,
)


def test_font_matches_audited_source() -> None:
    # Given: the retained upstream font.
    path = SOURCE / "bongocat.woff"
    # When: computing its content digest.
    actual = sha256(path)
    # Then: it is the exact audited artifact.
    assert actual == FONT_SHA256


@pytest.mark.parametrize("pair,changed_half", [("dc", 0), ("ba", 1)])
def test_strike_changes_only_its_paw_half(pair: str, changed_half: int) -> None:
    # Given: the original idle glyphs, rendered from their source outlines.
    font = load_font()
    idle = render_frame("bc", font)
    # When: one strike pose is rendered.
    strike = render_frame(pair, font)
    # Then: only the expected half changes, and the pose remains inside canvas.
    difference = ImageChops.difference(idle, strike).getchannel("A")
    bounds = difference.getbbox()
    assert bounds is not None
    if changed_half == 0:
        assert bounds[2] <= 136
    else:
        # The original c glyph has a -2/1024 em left sidebearing (one raster pixel).
        assert bounds[0] >= 135
    alpha = strike.getchannel("A")
    assert alpha.getextrema() == (0, 255)
    content = alpha.getbbox()
    assert content is not None
    assert 0 < content[0] < content[2] < CANVAS[0]
    assert 0 < content[1] < content[3] < CANVAS[1]


@pytest.mark.parametrize("name", ["idle", "left", "right"])
@pytest.mark.parametrize("pixels", [CELL_PIXELS, COMPACT_PIXELS])
def test_cell_samples_preserve_png_alpha(name: str, pixels: tuple[int, int]) -> None:
    # Given: a published transparent PNG.
    with Image.open(OUTPUT / f"{name}.png") as image:
        assert image.mode == "RGBA"
        assert image.size == CANVAS
        # When: converting to the cell renderer contract.
        cells = cell_frame(image, pixels)
        # Then: decoded rows equal the filtered source alpha byte for byte.
        expected = image.getchannel("A").resize(pixels, Image.Resampling.LANCZOS)
        assert bytes.fromhex("".join(cells["alpha_rows"])) == expected.tobytes()
        assert len(cells["monochrome_rows"]) == pixels[1] // 2
        assert all(len(row) == pixels[0] for row in cells["monochrome_rows"])


@pytest.mark.parametrize("filename,pixels", [
    ("cells.json", CELL_PIXELS), ("compact-cells.json", COMPACT_PIXELS),
])
def test_manifest_and_cells_match_published_files(
    filename: str, pixels: tuple[int, int],
) -> None:
    # Given: generated metadata and published artifacts.
    manifest = json.loads((OUTPUT / "manifest.json").read_text(encoding="utf-8"))
    cells = json.loads((OUTPUT / filename).read_text(encoding="utf-8"))
    # When: reading actual file digests and deriving cell samples from the PNGs.
    actual = {name: sha256(OUTPUT / name) for name in manifest["outputs"]}
    derived = {}
    for name in ("idle", "left", "right"):
        with Image.open(OUTPUT / f"{name}.png") as image:
            derived[name] = cell_frame(image, pixels)
    # Then: no stale output, generator, or frame encoding can pass.
    assert actual == manifest["outputs"]
    assert manifest["generator_sha256"] == sha256(Path(__file__).with_name("generate_assets.py"))
    assert derived == cells["frames"]
    assert len({tuple(frame["monochrome_rows"]) for frame in derived.values()}) == 3


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-q"]))
