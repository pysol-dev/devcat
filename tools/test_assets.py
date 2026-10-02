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


@pytest.mark.parametrize("name", ["idle", "left", "right"])
def test_compact_pose_contacts_the_next_row(name: str) -> None:
    # Given: a runtime pose placed immediately above a terminal divider.
    cells = json.loads((OUTPUT / "compact-cells.json").read_text(encoding="utf-8"))
    # When: reading its bottom rendered row.
    bottom = cells["frames"][name]["monochrome_rows"][-1]
    # Then: visible ink reaches the lower half; no blank row or half-row remains.
    assert any(glyph in "▄█" for glyph in bottom)


@pytest.mark.parametrize("name", ["idle", "left", "right"])
def test_compact_bottom_row_preserves_outline_space(name: str) -> None:
    # Given: the bottom terminal cell row, consisting of two alpha samples.
    cells = json.loads((OUTPUT / "compact-cells.json").read_text(encoding="utf-8"))
    samples = bytes.fromhex("".join(cells["frames"][name]["alpha_rows"][-2:]))
    # When: counting visible ink in the bottom row.
    fill_ratio = sum(alpha >= 96 for alpha in samples) / len(samples)
    # Then: at least half stays open, rejecting a solid rectangular base.
    assert fill_ratio <= 0.5


@pytest.mark.parametrize("pair,start,stop", [("dc", 11, 16), ("ba", 0, 6)])
def test_compact_strike_preserves_unchanged_head_and_eye(
    pair: str, start: int, stop: int,
) -> None:
    # Given: idle and strike poses with one identical original glyph half.
    font = load_font()
    idle = cell_frame(render_frame("bc", font), COMPACT_PIXELS, "idle")
    # When: downsampling the strike into the runtime cells.
    strike = cell_frame(render_frame(pair, font), COMPACT_PIXELS, {"dc": "left", "ba": "right"}[pair])
    # Then: coverage remains registered (one alpha level of Lanczos seam rounding).
    for idle_row, strike_row in zip(idle["alpha_rows"][:7], strike["alpha_rows"][:7], strict=True):
        for idle_alpha, strike_alpha in zip(
            bytes.fromhex(idle_row)[start:stop], bytes.fromhex(strike_row)[start:stop], strict=True,
        ):
            assert abs(idle_alpha - strike_alpha) <= 1
            assert (idle_alpha >= 96) == (strike_alpha >= 96)


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
        cells = cell_frame(image, pixels, name)
        # Then: decoded rows equal the filtered source alpha byte for byte.
        source = image.getchannel("A")
        if pixels == COMPACT_PIXELS:
            # Fixed source coordinates lock upper head/body/eyes to one mapping.
            expected = Image.new("L", (16, 8))
            expected.paste(source.crop((0, 8, 272, 96)).resize((16, 7), Image.Resampling.LANCZOS))
            top, bottom = {"idle": (100, 104), "left": (124, 133), "right": (127, 135)}[name]
            expected.paste(source.crop((0, top, 272, bottom)).resize((16, 1), Image.Resampling.BOX), (0, 7))
            actual = bytes.fromhex("".join(cells["alpha_rows"]))
        else:
            expected = source.resize(pixels, Image.Resampling.LANCZOS)
            actual = bytes.fromhex("".join(cells["alpha_rows"]))
        assert actual == expected.tobytes()
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
            derived[name] = cell_frame(image, pixels, name)
    # Then: no stale output, generator, or frame encoding can pass.
    assert actual == manifest["outputs"]
    assert manifest["generator_sha256"] == sha256(Path(__file__).with_name("generate_assets.py"))
    assert derived == cells["frames"]
    assert len({tuple(frame["monochrome_rows"]) for frame in derived.values()}) == 3


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-q"]))
