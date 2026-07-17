# Bundled fonts

- `NotoSans-Regular.ttf` — base UI font (Latin / Cyrillic / Greek),
  compiled into the binary via `include_bytes!` (`src/ui/fonts.rs`).
- `NotoSansCJKsc-Regular.otf` — CJK fallback, **not** compiled in:
  lazily loaded at runtime the first time CJK text appears (native:
  read from this directory; wasm: fetched from the deploy origin).

Both are Google Noto fonts, licensed under the SIL Open Font License 1.1
(https://openfontlicense.org). Source: https://notofonts.github.io /
https://github.com/notofonts/noto-cjk.
