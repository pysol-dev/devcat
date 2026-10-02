# BongoCat assets

Run `uv run tools/generate_assets.py` from the repository root. The script has
pinned Pillow and fontTools dependencies and verifies the original WOFF hash
before rendering. No installed system font is used. Run the artifact checks with
`uv run tools/test_assets.py`.

The transparent `generated/{idle,left,right}.png` frames are 272 × 152 RGBA.
Original glyphs occupy adjacent 128-pixel em boxes at x=8 and x=136, with a fixed
baseline y=128. Pairing is idle `bc`, left `dc`, right `ba`. Ink is white;
renderers can tint it to the terminal foreground while retaining alpha coverage.
The original icon is a cat leaning on a bar. No replacement illustration or
drum artwork is used.

`generated/preview.png` shows idle, left, right in columns. Each theme has the
original raster on top and an enlarged cell approximation below. The preview
uses dark `#1e1e1e` with `#f0f0f0` ink, and light `#f5f5f5` with `#1e1e1e` ink.

## Cell format, version 1

`generated/cells.json` is a compact, dependency-free input for a Rust renderer:

- 34 columns × 10 terminal rows; each terminal cell contains two square samples.
- Each pose has 20 `alpha_rows`, each containing 34 bytes encoded as 68 lowercase
  hexadecimal characters. Rows go top to bottom; samples go left to right.
- For cell `(x,y)`, the upper alpha is sample x of row `2*y`; the lower alpha is
  sample x of row `2*y+1`. Zero means leave the host background visible.
- For truecolor, blend each sample against the underlying cell background using
  `(foreground * alpha + background * (255-alpha) + 127) / 255` per RGB channel,
  with integer division. Emit `▀` with the upper RGB as foreground and lower RGB
  as background. Skip a cell when both samples are zero. Use the terminal theme's
  foreground for contrast; `foreground_rgb` records the source white ink.
- `monochrome_rows` contains ten 34-character rows of spaces, `▀`, `▄`, and `█`,
  thresholded at alpha 96. Draw using the terminal foreground and host background.
  These rows preserve all three paw states without requiring truecolor.

For runtime use in small terminal layouts, prefer `generated/compact-cells.json`:
it has the same schema at **16 columns × 4 rows**, with eight alpha rows of sixteen
bytes (32 hex characters each), and four monochrome rows of sixteen characters.
Every pose uses the **same upper source coordinates and filtering**. Original
pixels y=8..95 become the first seven samples through Lanczos resizing. The eighth
sample is an area average (BOX filter) of a narrow original contact strip: idle
y=100..103, left y=124..132, right y=127..134. Sampling the idle baseline only once
keeps its line thin; averaging narrow paw/droplet strips keeps strikes sparse.
Only that final half-cell uses pose-specific contact registration. Head/body/eye
coordinates and scale remain identical across all three frames.
Its lowest visible pixel reaches the bottom half of the last cell, so placing
the frame immediately above a divider leaves no empty row or half-row. The
compact contact band compresses strike marks; the raster PNGs retain the full
original outlines at their common source baseline.
`generated/compact-preview.png` compares original art with the exact thresholded
monochrome approximation touching a divider on both themes. `cells.json` remains
the larger reference variant with its original canvas padding.

The cells are an approximation of the original font. Actual terminal rendering,
font metrics, placement, background restoration, and host compatibility require
later renderer tests. The preview is not evidence of a terminal integration.

`generated/manifest.json` records source and output SHA-256 hashes, generator
hash, geometry, glyph mapping, palette, and the FreeType version. Byte-identical
reproduction requires the recorded rasterizer toolchain. PNGs contain no timestamps.
