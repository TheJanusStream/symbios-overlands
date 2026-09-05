# Bundled fonts

- `NotoSans-Regular.ttf` — base UI font (Latin / Cyrillic / Greek),
  compiled into the binary via `include_bytes!` (`src/ui/fonts.rs`).
- `NotoSansCJKsc-Regular.otf` — CJK fallback, **not** compiled in:
  lazily loaded at runtime the first time CJK text appears (native:
  read from this directory; wasm: fetched from the deploy origin).

Both are Google Noto fonts, licensed under the SIL Open Font License 1.1
(https://openfontlicense.org). Source: https://notofonts.github.io /
https://github.com/notofonts/noto-cjk.

## Two recorded decisions about what is *not* here

**Simplified Chinese is the only CJK regional cut we ship (#1262 f373).**
`NotoSansCJKsc` carries the code points for Japanese and Korean, so
neither renders as tofu — but Han unification means the several hundred
unified ideographs whose shapes differ between the regions are drawn in
Chinese letterforms for a Japanese or Korean reader. Fixing it means
shipping the JP and KR cuts as siblings and picking between them from
the sighted text (kana ⇒ JP, hangul ⇒ KR, otherwise SC), which is
roughly another 48 MB of assets to deploy and cache. That is a large
cost for a legibility papercut, so the app ships SC alone. This is a
decision, not an oversight; revisit it if the asset budget changes.

**No Hebrew, Arabic, Thai or Indic faces (#1262 f360).** Nothing in the
bundle or in egui's embedded tail covers them, so they are empty boxes
for the whole session. The app now names the gap to the user rather than
looking broken — `UNSUPPORTED_SCRIPTS` in `src/ui/fonts.rs`, whose rows
are probed against the real charmaps by a test, so a row cannot outlive
the gap it describes. Adding the faces is only half the work: epaint
0.35 has no bidirectional reordering, so an RTL face would render a
mixed Arabic/Latin line in the wrong segment order. Read the "What this
module cannot do" section of `src/ui/fonts.rs` before adding one.
