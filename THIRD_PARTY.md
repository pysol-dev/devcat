# Third-party artwork

## BongoCat VS Code icon font

Source: [kitgore/BongoCat](https://github.com/kitgore/BongoCat/tree/20e59c96cd25e4dcc098a5ab908bb27cdd4caabc),
commit `20e59c96cd25e4dcc098a5ab908bb27cdd4caabc`.

Retained without modification:

- `assets/source/bongocat.woff`
- `assets/source/bongocat-theme.json`
- `assets/source/LICENSE.bongocat`

Font SHA-256: `5b9bc74991b8abdb04785e4ef16db86bef867d5b19fa60444dbca11230224225`.

The [upstream extension](https://github.com/kitgore/BongoCat/blob/20e59c96cd25e4dcc098a5ab908bb27cdd4caabc/src/extension.ts)
uses left-up/right-up, left-down/right-up, and left-up/right-down pairs. The
retained theme maps these to `bc`, `dc`, and `ba`. `tools/generate_assets.py`
converts those exact glyph outlines into transparent PNGs and smaller cell
approximations. Generated artwork and previews retain this attribution and
license. No hand-drawn substitute artwork is included.

### Original license

MIT License

Copyright (c) 2023 ben

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
