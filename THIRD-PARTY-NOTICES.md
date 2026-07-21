# Third-party notices

WinRemap itself is [MIT](LICENSE) — Copyright (c) 2026 Daiki Suganuma.

This file lists third-party material whose license requires its notice to
travel with the distributed binary. Rust dependencies are not repeated here:
they are declared in `Cargo.toml`, resolved in `Cargo.lock`, and each carries
its own license text in the crate source.

---

## Bootstrap Icons

The tray menu icons (`gear`, `arrow-clockwise`, `card-list`, `box-arrow-right`)
are from [Bootstrap Icons](https://github.com/twbs/icons).

The SVG sources are vendored under `assets/icons/`, and `build.rs` rasterizes
them to 16x16 RGBA at build time ([ADR 0040](docs/v0.2/decisions/0040-menu-icons-rasterized-at-build-time.md)).
**Those pixels are embedded in `winremap.exe`**, which is why the notice below
ships with the binary.

```
The MIT License (MIT)

Copyright (c) 2019-2024 The Bootstrap Authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

---

## Not included here

- **Keyhac / fakeymacs / xremap** — acknowledged in the README as design and
  workflow references. No code from them is present in WinRemap, so their
  licenses do not attach to this distribution (AGENTS.md forbids porting logic
  from them without adding the notice here)
- **kanata** — LGPL-3.0. Deliberately not referenced at all, in code or design
  (AGENTS.md)
