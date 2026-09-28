# Re-authored trees

Builders for the Understory's two re-authored L-system trees (session 878)
and Ashmere's two, kept because the grammars carry engine knowledge that
took hours of rendering to find, and a builder in a scratchpad is gone
when the session ends. Each was made by one sub-agent designing against
pictures, a second criticising the result, and a third fixing what the
critic found (see [../../developing.md](../../developing.md), "Delegating
to a sub-agent"). The oak and the apple were made for Ashmere, a Norfolk
manor village of about 1300, and judged in its evening light; the apple is
the oak's grammar scaled to a fruit tree.

| Builder | Tree | Cost | Rebuild the shipped tree |
|---|---|---|---|
| `spruce.py` | `dark_conifer`: a conical, tiered dark spruce | 3,788 triangles, 2 parts | `python3 spruce.py d dark_conifer.json` |
| `birch.py` | `pale_birch`: a silver birch, white marked trunk, hanging leaf strands | 4,316 triangles, 2 parts | `python3 birch.py` |
| `oak.py` | an open-grown English oak: a 3.1 m bole, low crooked limbs, fissured grey-brown bark, a broad lumpy dark-green crown 21 x 17 m, a sixth of it bronzing; preset `y` is a 9 m young oak (3,614 triangles) | 7,674 triangles, 3 parts | `python3 oak.py` |
| `apple.py` | an old croft apple: short leaning bole, low rounded crown 7 x 5.6 m, 65 solid apples, red-flushed and a few green | 3,890 triangles, 3 parts | `python3 apple.py` |

Each writes one `network.symbios.gen.lsystem` generator in wire form, to
set with `room set /generators/<name> --file <out>`. The docstrings list
every variant tried and why it was rejected; the notes at each parameter
say what it does to the picture. What the spruce and the birch learned,
in short, is in [../../region.md](../../region.md) under "Planting:
scatters"; the oak's engine notes are at the end of its docstring (a
branch that starts at its own width, a trunk left as one tube, the
texture envelope the sanitiser clamps to, the Leaf texture's vein cast).

The saved record differs from a fresh build only where the sanitiser drops
a default (an identity scale, default texture fields) or pulls a texture
field into its envelope: compare after a `room set`, not before.

## In the catalogue

Four of these trees are catalogue entries now (#1496), each the generator
saved in Ashmere's room record, copied into `src/catalogue/items/plants/`:

| Builder | Catalogue entry |
|---|---|
| `birch.py` | `lsys_birch` - it replaced the #910 birch there, so the seeded worlds' birch stands grow it too |
| `oak.py` | `lsys_oak` |
| `oak.py` preset `y` | `lsys_young_oak` |
| `apple.py` | `lsys_apple` |

Ashmere's other plants joined them: `lsys_yew` is preset `y` dressed as a
yew (a Needle card, a red-brown bark), and `lsys_hazel` and `lsys_gorse` are
the catalogue's own `lsys_bush` grammar in Ashmere's materials, with coarser
tubes, each stretched to its own size by a root transform. A fresh build of
each builder draws the same tree as its entry - the files differ only in
defaults a record leaves out, and the pictures are pixel-identical - but a
change made here does not reach the catalogue by itself: judge it, save it,
then copy the saved record into the entry, and check that `render --dump
--catalogue <slug>` equals the record field for field.
