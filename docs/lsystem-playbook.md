# L-system design playbook

How to take a species request ("we need a birch") to a high-quality grammar
quickly. The goal is never "draws a plant that looks good" — it is **simulates
the plant's morphogenesis well enough that the four mileage levers fall out for
free**:

| Lever | What varies | Mechanism |
|---|---|---|
| **Age** | one individual over time | iteration count |
| **Individual** | specimens of one species | RNG seed → stochastic rules |
| **Re-skin** | species/season/biome variants | materials + the finalization pass |
| **Simulation** | related species from one grammar | `#define` constants |

A grammar that models growth gets all four. A grammar that draws a silhouette
gets none — it looks identical at every age, every seed, and can only be
re-coloured.

Sources: Prusinkiewicz & Lindenmayer, *The Algorithmic Beauty of Plants*
([ABOP](https://algorithmicbotany.org/papers/abop/abop.pdf)); Prusinkiewicz,
Karwowski & Lane, *[The L+C modeling language](https://algorithmicbotany.org/papers/sigcourse.2003/4-1-L+C.pdf)*;
Kurth (1994) on interpretation rules.

---

## 1. Engine capabilities (symbios 1.5 + symbios-turtle-3d 0.5)

Turtle symbols: `F` draw · `f` move (no draw, **no tropism**) · `+`/`-` yaw ·
`&`/`^` pitch · `\`/`/` roll · `|` turn around · `$` roll-to-horizontal ·
`!` **set** width (absolute, not a multiplier) · `[`/`]` push/pop ·
`~(id,scale)` spawn prop · `,` set material · `'` set colour · `;` set UV scale.

Rule syntax — `[label:] [prob :] [left <] pred [> right] [: cond] -> successor`:

```
a1: 0.45 : A(l,w) : l > 0.05 -> !(w)F(l)[&(52)B(l*0.55,w*0.6)]/(137.5)A(l*0.95,w*0.9)
```

Supported and verified: stochastic weights, **weights and guards on the same
rule**, parametric modules, context-sensitivity (`A < B > C`) with
`#ignore: + - /`, `#define` constants (inlined at compile time — free),
and a separate **finalization ruleset**.

**Not supported:** the ABOP cut symbol `%` (branch self-pruning — how ABOP does
petal fall, frond shedding, autumn leaf drop). If a species needs organ
abscission, that is an upstream feature request.

### Two traps particular to this engine

- **`age` is per-rewrite, not per-plant.** We now advance the state clock once
  per derivation step (`lsystem.rs`), but `derive` re-stamps every rewritten
  module, so `age` counts steps since a module was *last rewritten*. It is
  useful only for dormant modules whose guard hasn't opened yet (bud break,
  flowering onset). **For whole-plant age, carry your own counter:**
  `A(l,w,n) -> ... A(l*r,w*wr,n+1)`.
- **`elasticity` is global to the generator.** There is no per-branch override,
  so you cannot make twigs droop while the trunk stays stiff by tuning it. See
  §4 — subdivide instead.

---

## 2. Node rewriting vs edge rewriting

**Node rewriting** replaces a non-drawn *marker* at a vertex (`A`, `S`, `D`)
which re-emits itself: `A(l,w) -> !(w)F(l)[...]A(l*r,w*wr)`. The marker **is the
meristem** — the biological growing tip. Growth is subapical (new material
appears only at tips), which is what real plants do.

**Edge rewriting** replaces a *drawn segment*, preserving its endpoints and
polarity: `F -> F[+F]F[-F]F`. Every existing segment re-subdivides each step, so
the whole structure refines at once. This violates subapical growth — ABOP
concedes it "produces an acceptable visual effect in a still picture" but it is
not development.

**Rule of thumb:** anything with a trunk and a crown (trees, palms, bamboo,
shrubs) → **node rewriting**. Ferns, fractal filler, moss, coral, lichen →
**edge rewriting** is acceptable and often more compact. Every tree in this
catalogue is node-rewriting; that is correct and should stay.

---

## 3. Growth: make iterations mean age

**This is the single highest-leverage technique.** ABOP §1.10.3 proves two
grammars give *identical geometry* but different developmental meaning:

```
# Fractal — a tip's final size is computed at birth. Looks the same at every n.
A(s) -> F(s)[+A(s/R)][-A(s/R)]

# Developmental — new segments are born at BASE size, and everything already
# placed keeps growing. Iteration count IS age.
A     -> F(1)[+A][-A]
F(l)  -> F(l*lr)          # retrospective elongation
!(w)  -> !(w*vr)          # retrospective thickening
```

Prefer the second ("retrospective growth"). Old wood ends up genuinely longer
and thicker than new wood, the base accumulates girth, and a sapling and a
mature specimen are *the same ruleset at different `n`*. `lsys_ternary_props`
and `lsys_bush` use it.

The alternative (**prospective**) carries explicit state and is needed when
growth must saturate or be non-monotonic:

```
d1: D(l,n) -> !(0.02)F(l)D(l + k*(lmax-l), n+1)     # sigmoid approach to lmax
```

Geometric length decay `l*r^n` totals `l0/(1-r)` — use that to bound a crown.

**Taper within one internode** isn't expressible via `!()` alone (one radius per
node). Emit sub-segments: `F(l/3)!(w*0.93)F(l/3)!(w*0.86)F(l/3)`. The same trick
in reverse — width *pulses* — gives bamboo its nodal rings.

### Width: the pipe model (da Vinci's rule)

Summed daughter cross-section equals the parent's: `w_parent^e = Σ w_child^e`.

- `e = 2.0` for conifers/excurrent forms; `e = 2.3–2.6` for broadleaves (fatter
  trunks relative to twigs — reads better at game distances).
- Symmetric n-way split, prospective: `w_child = w * n^(-1/e)` → binary `0.707`,
  ternary `0.577`.
- Retrospective: conservation holds when `vr = n^(1/e)` → binary `√2 ≈ 1.414`,
  ternary `√3 ≈ 1.732`. Going *under* the pipe value reads whippy/shrubby;
  going at or above it reads load-bearing.

Use these instead of hand-tuned tapers — it is what makes a trunk look like it
carries its crown.

---

## 4. Tropism, droop, and lean

Applied after each `F` (never after `f`):

```
angle = elasticity · |H × T|        axis = normalize(H × T)
```

Three consequences that dominate everything:

1. **Bend is per drawn segment and independent of segment length.** So
   **segment count is the real droop knob.** A whip drawn as 12×`F(0.15)` droops
   six times as hard as the same length drawn as 2×`F(0.9)`. Since elasticity is
   global, *this* is how you make one organ droop more than another: subdivide
   it, don't retune elasticity.
2. **Max bend is `elasticity` radians per segment** (at H⊥T). `0.05` ≈ 2.9°/seg
   (trunks), `0.3` ≈ 17°/seg (fern fronds).
3. **A vertical heading is a fixpoint.** `H × T = 0` when `H ∥ ±Y`, so a
   perfectly vertical axis *never* bends, at any elasticity.

### The two failure modes of the fixpoint

- **Silent no-op:** you raise elasticity to droop a vertical shoot; nothing
  happens.
- **Runaway shepherd's crook:** you add a per-segment pitch wander to break the
  symmetry — and because bend ∝ `sin θ`, the more it leans the harder it bends.
  Positive feedback curls the tip into a hook. This bit the palm redesign.

**Controlled lean = one-shot tilt at the base, then let tropism integrate a
smooth arc.** Never a per-segment wander:

```
g1: 0.4 : G -> T C          # vertical — stays dead straight (fixpoint)
g2: 0.3 : G -> &(3)T C      # 3° base tilt — gentle arc over the whole trunk
g3: 0.3 : G -> ^(3)/(40)T C
```

**Weeping forms** are mechanical failure, not inverted tropism: normal `T`, huge
segment counts, tiny widths. **Negative gravitropism** (`T = (0,+1,0)`) makes
`+Y` an attractor — drooped shoots curve back up; good for pioneers.

**Plagiotropism** (fir laterals holding horizontal) can't come from a global
`T = ±Y`. Two workarounds: cancel the droop with a small per-segment up-pitch
(`^(3)` cancels `e≈0.05`), or use **`$`**, which rolls the turtle so its left
vector is horizontal — preserving heading, flattening the branch *plane*. `$` is
the ABOP idiom for flat crowns and is why the monopodial `b1`/`c1` rules use it.

---

## 5. Architecture: the vigour ratio

The ratio `r2/r1` (lateral contraction ÷ leader contraction) is the master knob
for tree architecture:

| Form | `r1` | `r2` | `r2/r1` | Reads as |
|---|---|---|---|---|
| Monopodial / excurrent | 0.9–0.95 | 0.5–0.65 | ~0.6 | Spruce, fir, young birch — conical, strong leader |
| Weak apical control | 0.85 | 0.75 | ~0.9 | Oak, maple |
| Sympodial | leader terminates | two ~equal | 1.0 | Dichotomous forks, lilac |
| Deliquescent | 0.75 | 0.85 | >1.1 | Trunk dissolves into the crown |

**Acrotony vs basitony — the tree/shrub switch.** Where the *biggest* laterals
sit. Retrospective grammars give **basitony for free** (early laterals have had
more steps to grow), which is why naive L-system trees look shrubby. To get a
tree you must actively make lateral vigour *rise* with node index:

```
# acrotony (tree)
a1: A(l,w,n) -> !(w)F(l)[&(45)B(l*(0.4+0.05*n), w*0.7)]/(137.5)A(l*0.92,w*0.88,n+1)
# basitony (shrub) — lowest branches longest, domed silhouette
a1: A(l,w,n) -> !(w)F(l)[&(60)B(l*(0.9-0.08*n), w*0.7)]/(137.5)A(l*0.9,w*0.88,n+1)
```

Mesotony (biggest branches mid-crown, very common in broadleaves) is a clamped
parabola in `n`.

**Deliquescence with age** — young excurrent, old decurrent, as real trees do —
is a guard on the counter:

```
a1: A(l,w,n) : n < 6  -> ... strong leader ...
a2: A(l,w,n) : n >= 6 -> ... leader replaced by two equals ...
```

---

## 6. Phyllotaxis: get 137.5° exactly right

The golden angle is `360·τ⁻² ≈ 137.5°`. **ABOP Fig 4.2 shows 137.3 / 137.5 /
137.6 producing visibly different parastichies** — a tenth of a degree collapses
the spiral packing into radial gaps. Divergence angle is species identity:
jitter it by **at most ±1°**, and put the stochastic budget on topology instead.

| Arrangement | Roll | Examples |
|---|---|---|
| Spiral / alternate | `/(137.5)` | most trees — the default |
| Distichous (2-ranked) | `/(180)` | grasses, elm, fern pinnae |
| Decussate (opposite) | pairs, then `/(90)` | maple, ash, mints |
| Tristichous | `/(120)` | sedges |
| Whorl of n | `n` organs ~`360/n` apart | palm crowns, araucaria |

**Never use a repeating roll at a rational fraction of 360°** (90, 120, 72, 60,
45): successive nodes stack into visible vertical columns with radial gaps
between — azimuth-notch resonance. Whorls need `360/n`, so jitter each roll ±3–6°
(the palm crown does this) and offset each tier by a non-dividing angle. A
terminal, non-repeating ring (a single flower's petals) is safe at exactly
`/(72)` because it never iterates.

---

## 7. Stochastic variation: spend it on topology

ABOP §1.7 is explicit: *"Randomization of the interpretation alone has a limited
effect… the underlying topology remains unchanged. In contrast, stochastic
application of productions may affect both the topology and the geometry."*

Two specimens with jittered angles read as *the same model rotated*. Two with
different topology read as **different individuals**. So:

- Put seed variance into **branch / stall / abort choices**, whorl counts, and
  fork arity — as the palm's `g1/g2/g3` (stance) and `c1/c2/c3` (frond count) do.
- A **stall** alternative (`S -> S`, no growth this step) is what produces
  natural irregularity of internode spacing and crown density.
- Angles: Gaussian, σ ≈ 10–15% of the mean. Lengths: log-normal, σ ≈ 15–25%.
- Divergence angle: near-deterministic (§6).
- **Stochastic rules for within-species variation; `#define` constants for
  between-species.** Don't blur the two.

---

## 8. Finalization = homomorphism (separating logic from appearance)

Our finalization ruleset is a real, named technique: **interpretation rules**,
introduced by Kurth (1994), spelled `homomorphism:` in cpfg/L-Py and
`interpretation:` in L+C. From the L+C paper: interpretation rules *"are applied
'on the side', producing modules that are passed to the graphical part of the
modeling program, and discarded once they have been interpreted"* — they do not
affect subsequent derivation.

**Therefore: the growth grammar should emit only abstract markers** — `L` leaf
site, `K` bud, `V` flower site, `P` leaflet pair — and finalization decides what
they *become*:

```
# summer                  # blossom                  # winter bare
L -> ,(1)[+(50)~(0,14)]   L -> ,(3)[~(1,9)]          L ->
                                                     K -> ,(0)~(2,4)
```

One skeleton, three seasons, zero grammar duplication. This is the re-skin lever
and it is why organ expression must **not** be inlined into growth rules.

Two mechanical notes:
- Finalization **clears the ruleset** and runs exactly **one** `derive(1)` — it
  is a single non-recursive pass. One marker, one expansion, no chaining.
- Because of that, **give every marker a finalization terminal**. A marker
  created on the last growth step otherwise renders nothing — that was a real
  silent gap in the palm (last-iteration leaflets were invisible).

**Material slot convention** (keep it stable across all species so palettes and
biome tinting stay interchangeable): `0` = bark, `1` = twig/leaf-cluster,
`2` = leaf, `3` = flower/fruit.

Worth adding later: **decomposition rules** ("consists of", applied recursively
*within* a step and affecting derivation) are a distinct tier from interpretation
rules ("develops into"). Having both is how you share an organ cluster across
many species without copy-paste.

---

## 9. Failure modes

| Symptom | Cause | Fix |
|---|---|---|
| Crown balls up into a solid blob | Branch count grows `n^k` while length decays `r^k` | Keep `n·r³ < 1`: ternary `r < 0.693`, binary `r < 0.794`. Or add a stochastic terminate rule. |
| Visible vertical columns / radial gaps | Divergence at a rational fraction of 360° | Use 137.5 (§6); jitter whorl rolls. |
| Droop knob does nothing | Vertical heading is a tropism fixpoint | Tilt off vertical, or increase segment count. |
| Tip curls into a shepherd's crook | Per-segment pitch wander + `sin θ` feedback | One-shot base tilt only. |
| Bare sticks at low iteration counts | Markers never expressed | Finalization terminal for *every* marker. |
| Tree looks like a shrub | Retrospective growth gives basitony free | Make lateral vigour rise with node index (§5). |
| Trunk looks too thin to hold the crown | Hand-tuned taper below pipe-model | `vr = n^(1/e)` (§3). |
| Age sweep plateaus (all old ages identical) | Growth is fractal, not developmental | Adopt retrospective growth (§3). |
| Self-intersection | Context-free grammars have no spatial awareness | Reduce angle spread, add roll jitter. True fixes (space colonization, environmental sensitivity) are out of scope; foliage hides most of it. |

---

## 10. Recipe: species request → grammar

1. **Classify the architecture.** Monopodial / sympodial / deliquescent /
   rosette / clump-forming? Pick `r1`, `r2` from §5.
2. **Choose rewriting mode.** Trunk-and-crown → node; fractal filler → edge (§2).
3. **Skeleton first, no organs.** Get the silhouette and the age sweep right
   with bare wood. Use retrospective growth (§3) so `n` = age.
4. **Set width** from the pipe model (§3).
5. **Set phyllotaxis** (§6) — exactly 137.5 unless the species is distichous,
   decussate or whorled.
6. **Add tropism** — pick elasticity for the *stiffest* organ, then control
   per-organ droop by segment subdivision (§4).
7. **Add stochastic alternatives on topology** — 2–3 per recursive rule (§7).
8. **Only now add organs**, as bare markers, expressed in finalization (§8).
9. **Render the age sweep** (`--ages 2,3,5,7,10`) and iterate. Then render two
   or three seeds at the default age to confirm individuals differ.
10. **Record the tuning** in the module doc comment — future readers need the
    *why*, especially for angle constants.

### Verification loop

```bash
cargo run --bin render -- --catalogue lsys_birch --ages 3,5,7,9,11
cargo run --bin render -- --catalogue lsys_birch --dump > /tmp/x.json   # edit + re-render
cargo run --bin render -- --generator /tmp/x.json --ages 3,5,7,9,11     # no recompile
```

The `--generator` loop avoids a rebuild per grammar edit — use it while tuning,
then bake the final grammar back into the catalogue entry.
