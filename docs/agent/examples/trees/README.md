# Re-authored trees

Builders for the Understory's two re-authored L-system trees (session 878)
and Ashmere's two, kept because the grammars carry engine knowledge that
took hours of rendering to find, and a builder in a scratchpad is gone
when the session ends. Each was made by one sub-agent designing against
pictures, a second criticising the result, and a third fixing what the
critic found (see [../../developing.md](../../developing.md), "Delegating
to a sub-agent"). The oak and the apple were made for Ashmere, a Norfolk
manor village of about 1300, and judged in its evening light; the apple is
the oak's grammar scaled to a fruit tree. The cherry and the plane were made
for the catalogue itself (#1763, #1764, the owner's choices of 2026-10-23):
the apple's grammar reshaped into a cherry's vase in blossom, the oak's
into a plane's high dome.

| Builder | Tree | Cost | Rebuild the shipped tree |
|---|---|---|---|
| `spruce.py` | `dark_conifer`: a conical, tiered dark spruce | 3,788 triangles, 2 parts | `python3 spruce.py d dark_conifer.json` |
| `birch.py` | `pale_birch`: a silver birch, white marked trunk, hanging leaf strands | 4,316 triangles, 2 parts | `python3 birch.py` |
| `oak.py` | an open-grown English oak: a 3.1 m bole, low crooked limbs, fissured grey-brown bark, a broad lumpy dark-green crown 21 x 17 m, a sixth of it bronzing; preset `y` is a 9 m young oak (3,614 triangles) | 7,674 triangles, 3 parts | `python3 oak.py` |
| `apple.py` | an old croft apple: short leaning bole, low rounded crown 7 x 5.6 m, 65 solid apples, red-flushed and a few green | 3,890 triangles, 3 parts | `python3 apple.py` |
| `cherry.py` | a Japanese flowering cherry in full blossom: a vase of four scaffolds on a 1.5 m bole, 5.9 m tall and 7.3 m across, pink blossom along the limbs, lenticelled red-brown bark; presets `w` (white blossom) and `s` (summer green) are its re-skins | 3,308 triangles, 3 parts | `python3 cherry.py` |
| `plane.py` | a London plane: a 3.8 m bole of camouflage bark, five ascending limbs, a high broad dome of big lobed leaves 14.7 m tall and 16.7 m across; `autumn` and `blossom_pale` are its re-skins | 7,648 triangles, 2 parts | `python3 plane.py` |

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

Seven of these trees are catalogue entries now, copied into
`src/catalogue/items/plants/`: four from Ashmere's room record (#1496), the
spruce from the Understory's (#1708), and the cherry and the plane from
their builders' own output (#1763, #1764):

| Builder | Catalogue entry |
|---|---|
| `spruce.py` | `lsys_monopodial_tree` - it replaced the ABOP Fig 2.6 conifer there, keeping the slug and its `pine`, `larch_gold` and `frosted` re-skins, so the seeded worlds' conifer stands grow it too; the entry grows it at iterations 11, a whorl younger than the Understory's 12, so a seeded stand's age roll still has a step above it |
| `birch.py` | `lsys_birch` - it replaced the #910 birch there, so the seeded worlds' birch stands grow it too |
| `oak.py` | `lsys_oak` |
| `oak.py` preset `y` | `lsys_young_oak` |
| `apple.py` | `lsys_apple` |
| `cherry.py` | `lsys_flowering_tree` - it replaced the #910 blossom tree there, keeping the slug, so the seeded worlds' blossom stands and Berlin's cherries, apples, pears, thorns and rowans grow it (the pears, apples and thorns in its `white` re-skin, the rowans in `summer`) |
| `plane.py` | `lsys_sympodial_tree` - it replaced the #910 sympodial broadleaf there, keeping the slug and its `autumn` and `blossom_pale` re-skins, so the seeded worlds' broadleaf stands and Berlin's planes, chestnuts, beeches and elms grow it |

Ashmere's other plants joined them: `lsys_yew` is preset `y` dressed as a
yew (a Needle card, a red-brown bark), and `lsys_hazel` and `lsys_gorse` are
the catalogue's own `lsys_bush` grammar in Ashmere's materials, with coarser
tubes, each stretched to its own size by a root transform. A fresh build of
each builder draws the same tree as its entry - the files differ only in
defaults a record leaves out, and the pictures are pixel-identical - but a
change made here does not reach the catalogue by itself: judge it, save it,
then copy the saved record into the entry, and check that `render --dump
--catalogue <slug>` equals the record field for field.
