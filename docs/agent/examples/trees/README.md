# Two re-authored trees

Builders for the Understory's two re-authored L-system trees (session 878),
kept because the grammars carry engine knowledge that took hours of
rendering to find, and a builder in a scratchpad is gone when the session
ends. Each was made by one sub-agent designing against pictures, a second
criticising the result, and a third fixing what the critic found (see
[../../developing.md](../../developing.md), "Delegating to a sub-agent").

| Builder | Tree | Cost | Rebuild the shipped tree |
|---|---|---|---|
| `spruce.py` | `dark_conifer`: a conical, tiered dark spruce | 3,788 triangles, 2 parts | `python3 spruce.py d dark_conifer.json` |
| `birch.py` | `pale_birch`: a silver birch, white marked trunk, hanging leaf strands | 4,316 triangles, 2 parts | `python3 birch.py` |

Each writes one `network.symbios.gen.lsystem` generator in wire form, to
set with `room set /generators/<name> --file <out>`. The docstrings list
every variant tried and why it was rejected; the notes at each parameter
say what it does to the picture. What they learned, in short, is in
[../../region.md](../../region.md) under "Planting: scatters".

The saved record differs from a fresh build only where the sanitiser drops
a default (an identity scale, default texture fields): compare after a
`room set`, not before.
