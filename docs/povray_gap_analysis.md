# POV-Ray port: gap analysis

What this ray tracer would need in order to render the scenes in
[`mschaef/povray-projects`](https://github.com/mschaef/povray-projects),
and a suggested order for the work. Texaco comes first, then xmastree,
then the rest.

- Written 2026-09-22.
- Based on `povray-projects` at `bce920d` and `raytracer-exp` on
  `ai-main` at `0b3da2b`, which includes the uncommitted `desugar.rs`
  work.
- POV-Ray standard-include values (`metals.inc`, `textures.inc`,
  `woods.inc`, …) were checked against the POV-Ray 3.7 distribution.
  The scenes were written for 3.1, but these presets are the same or
  close.

Size key: **S** = part of one session, **M** = about one session, and
**L** = a multi-phase plan in the style of the existing CLAUDE.md
plans.

---

## Status (2026-09-22)

- **Texaco: ported.** `scenes/texaco.lisp` and
  `scenes/texaco_frames.lisp`, with CSG added for it (CLAUDE.md
  history entries 39–41). Surfaces are untuned starting points.
- **Conventions confirmed:**
  - `scenes/pov_compass.lisp` confirms handedness; POV coordinates and
    `rotate` angles port unchanged.
  - The black-background reference GIFs were rendered without the
    white backdrop plane that the newest `texaco.pov` adds.
- **Next on the list:** xmastree (§5).

## 1. Summary

**Texaco** needs one new renderer feature: **CSG** (`difference`, and
`intersection` along with it). Everything else in the scene can be
mapped with a small POV-compatibility layer written in the SDL
(colors, corner-specified boxes, starting-point surfaces).

**Xmastree** reuses CSG and adds these gaps:

- A **BVH builder**: the tree is about 16,700 spheres plus about 140
  CSG ornaments.
- A **torus** primitive.
- **SDL randomness and math**: a seeded RNG, a Gaussian, `floor`, and
  vector helpers.
- **Procedural wood pigments**.
- **Light features**: shadowless lights, rectangular area lights, and
  an area light that is also a spotlight.

The wood and light items all have acceptable stand-ins for a first
port.

**The rest** fall into two groups:

- `ornament`, `braids`, `train`, `redball`, `nba` and `cpot` are cheap
  once Texaco and xmastree are done. They add a mesh import step, a
  two-colour checker pattern, and layered textures.
- The **snowman** scenes are much harder than everything else put
  together: blobs, normal perturbation, height fields, image maps,
  filtered (tinted) transparency, refraction, and brick patterns. They
  belong last.

CSG is the one architectural change at the root of all of this. Five
of the eight projects use `difference` or `intersection`, so it's the
right first investment.

### Decisions (2026-09-22)

- **Use this renderer's shading and light model.** The ports don't
  emulate POV's shading (gamma handling, POV-style `metallic`,
  `brilliance`, exact finish values). Scenes get tuned by eye later.
  That drops T4 and T5 and makes T3 optional (see §4).
- **Port functionally, not by textual expansion.** Things like the
  xmastree bead chains and `paths.inc` become functions that thread
  state through, not a translation of POV's macros and `#declare`
  mutation.
- **Order:** Texaco first, starting with CSG. Snowman is last.

---

## 2. What already maps cleanly

These POV-Ray features already have a direct equivalent. The only
work for them is in the port itself.

| POV-Ray | Here | Notes |
|---|---|---|
| `sphere`, `plane`, `cylinder` (closed) | `sphere`, `plane`, `cylinder` | |
| `box { <a>, <b> }` | `cuboid {:center :size}` | Needs a corner-to-center helper (see §3). Corners may be given in either order. |
| `cone { p0, r, p1, 0 }` | `cone` | Only when one radius is 0; swap ends if the apex comes first. Truncated cones are a gap (snowman). |
| `triangle` | `triangle` | Meshes of loose triangles, like `smokestack.inc`, are better converted to OBJ once (§6). |
| `union` (as grouping) | `group` + `with-surface` | "Innermost wins" matches how POV passes a texture down to children that have none. |
| `translate` / `rotate` / `scale` | same | Degrees → radians. The rotation matrices match POV's (see §3). |
| `camera { location look_at }` | `camera-looking-at … zoom 1.0` | POV's default camera (`direction 1`, `up 1`) is exactly zoom 1.0 (53.13° vertical FOV). |
| `camera { direction k*z }` | zoom `k` | Xmastree uses `direction 2*z`, so zoom 2.0. |
| `light_source { p color C }` | `light-point` | Neither POV nor this renderer has distance falloff by default. |
| `spotlight radius R falloff F` | `light-spot` inner = R, outer = F | Both are half-angles in degrees in POV. |
| `background` | `:background` | |
| `pigment { rgbt <r,g,b,t> }` | `:transparency t` | Without an `ior`, POV transmission doesn't bend light either, so the non-refractive model here matches exactly. |
| `finish { ambient reflection }` | `:ambient`, `:reflection` | `ambient 1` backdrops map directly. |
| `+A` anti-aliasing | adaptive sampler | |
| `#declare`, `#macro`, `#if`, `#switch`, `#while` | `def`, `defn`, `if`/`cond`, `map`/`range`/`reduce`/`recur` | Hand-porting is straightforward. See §3 for mutable state. |

---

## 3. Conventions for the port itself

These are decisions about how to port, not renderer features. Settle
them once, during the first port.

1. **Coordinates and handedness.** `Camera::looking_at` computes
   `right = up_hint × forward`. With up `+y` and forward `+z` that gives
   `right = +x`, which is POV's left-handed screen layout. POV's
   `rotate` matrices are the ordinary ones too. So POV coordinates and
   angles should carry over unchanged. **Check this with an asymmetric
   object before trusting it.** The red/green/blue arrow compass in
   xmastree, braids and train is a good test object, and porting it is
   worth doing on its own.

2. **Gamma.** POV 3.1 scenes without `assumed_gamma` treat pigment
   colours as display values, and this renderer works in linear light,
   so ported colours will come out lighter and less saturated. Per the
   decisions above, that's accepted: colours are used as written and
   adjusted by eye, with no conversion layer or "no gamma" mode.

3. **A POV compat library, `scenes/_pov.lisp`.** It grows with each
   port and holds:
   - Named colours from `colors.inc`.
   - `(box a b)`, which builds a cuboid from two corners.
   - Starting-point surfaces for the POV presets a scene uses
     (`F_MetalA`/`C`, `P_Gold3`, `P_Silver3`, `T_Silver_3C`,
     `T_Brass_3E`, `Chrome_Texture`, …), built from this renderer's own
     fields: colour, `:ambient`, `:light`, `:specular`, `:reflection`,
     and its own `:metallic`. These are rough first guesses to tune by
     eye, not a mapping of POV's finish model.

4. **Mutable state becomes threaded state.** POV ports lean on
   `#declare` mutation. The worst case is `paths.inc` in xmastree, a
   global path-position stack. Rewrite these as functions that take and
   return the path state (`reduce` over the steps). The SDL has no atoms
   and doesn't need them for this.

5. **Randomness won't match exactly.** Xmastree seeds `rand` with 700
   and draws ornament placement and bead jitter from it. With an
   ordinary seeded RNG in the SDL, the tree will look like the original
   but with different ornaments in different places. Matching POV
   exactly would mean copying POV 3.1's RNG and `rand.inc`'s
   `Rand_Gauss`. That's possible but a **Could** item (see open
   questions).

6. **Port by hand; don't write a translator.** Translating POV's scene
   language (macros that expand to partial blocks, `#declare`
   mutation, `#ifdef`, include paths) is a large project in its own
   right. There are eight small projects here, and hand ports read
   better in the SDL.

---

## 4. Texaco (priority 1)

**Scene.** `texaco/texaco.pov`, the final 640×480 still, renders
`texaco_hemi_logo`. The reference images are `texaco.gif` and
`texaco2.gif`: a red metallic hemispherical bowl with a silver star
straddling its rim plane, with a "T" cut through the star. The star is
reflected in the bowl.

**Geometry used:**

- `star = difference { cylinder, 5 × (union of 2 boxes, rotated/translated) }`
- `texaco_star = difference { star, union { box, box } }`, i.e. the T cut-out
- The star gets a non-uniform scale `<0.9, 0.9, 0.16>`, then a
  translate, then `rotate y*clock`.
- Bowl: `difference { sphere r=1, sphere r=0.999, cylinder }`, a
  0.001-thick hemispherical shell.
- An `ambient 1` white plane backdrop at z=10, a black background, one
  white point light, and the default camera at `<0,0,-2.2>`.

**Materials:** bowl `F_MetalC` + Red, star `F_MetalA` + White.
`F_MetalC` is `ambient .25, brilliance 4, diffuse .5, metallic,
specular .8, roughness 1/80, reflection .5`. `F_MetalA` is `ambient
.35, brilliance 2, diffuse .3, metallic, specular .8, roughness 1/20,
reflection .1`.

### Gaps

| # | Gap | Need | Size | Notes |
|---|---|---|---|---|
| T1 | **CSG: `difference` + `intersection`** | Must | L | See the design sketch below. Both the star and the bowl depend on it. The alternatives are worse: an extruded-polygon star mesh with the T hole triangulated by hand, plus a tessellated hemisphere. |
| T2 | `_pov.lisp` basics | Must | S | `box` from corners, `Red`/`White`/`Black`, starting-point metal surfaces. Port-side only. |
| T3 | Specular exponent on `Surface` | Could | S | The renderer has a hard-coded Phong power of 50. Not needed to emulate POV, but a `:specular-power` knob (default 50, so existing scenes stay byte-identical) would be a useful tuning control in this renderer's own model. Add it only if tuning Texaco calls for it. |
| ~~T4~~ | ~~POV-style `metallic`~~ | Dropped | — | Use this renderer's `metallic` as it is. |
| ~~T5~~ | ~~`brilliance`~~ | Dropped | — | Not part of this renderer's model. |
| T6 | Animation (`rotate y*clock`) | Could | S | It's already mostly possible: an `sdl_run` script can loop `render` + `save-png` over angles. Nice extras: `dotimes`/`doseq` sugar in `desugar.rs` and zero-padded file names. Reference: 24 frames, clock 0 → -180. |
| — | Thin-shell precision | Risk | — | The bowl is 0.001 thick, 10× `EPSILON`. Watch for acne and for the rim ring. If it shows up, thicken the shell a little in the port rather than changing `EPSILON`. |

### CSG design sketch (for T1)

This is a starting point for a proper phased plan in CLAUDE.md, not
the plan itself.

- **A new all-crossings query next to `hit_test`.** For example,
  `fn crossings(&self, ray, out: &mut Vec<Crossing>)`, where
  `Crossing { t, entering: bool, normal, surface }` and `t` ranges over
  the **whole line**, negative values included. The negative values are
  needed so a ray that starts inside a solid gets the right in/out
  state.
  - Solid primitives implement it: sphere, cuboid, cylinder, cone,
    torus later, and plane as a half-space with the solid on the side
    opposite the normal.
  - Triangles and meshes aren't solids and aren't allowed as CSG
    operands. The SDL should reject them with a clear error.
- **Wrapper nodes pass crossings through.**
  - `Transform` inverse-transforms the ray. `t` carries across unchanged
    because of the "don't renormalize" rule already in CLAUDE.md, which
    keeps paying off here. Normals go through `normal_xform`.
  - `Group` acts as a union and merges its children's crossings.
  - `Surfaced` fills in missing surfaces.
  - `Bounded` must do a whole-line slab test for this query, not the
    `t > 0` test used by the existing one.
- **A new node, `Shape::Csg(Box<Csg { op, a, b }>)`.** Its `hit_test`
  merges the two sorted crossing lists, tracks inside-A and inside-B,
  and returns the first `t > EPSILON` where the combined membership
  flips. Surfaces from a subtracted operand get **flipped normals**,
  which is the classic CSG bug to watch for.
  - The shadow walk, reflection and transmission all go through
    `hit_test`, so they work unchanged.
  - `bounds()`: difference → bounds(A), intersection → the overlap of
    the two bounds, union → both.
- **SDL surface.**
  - `(difference a b c …)` means A − (B ∪ C ∪ …), which is POV's n-ary
    form.
  - `(intersection a b …)`.
  - `(merge …)` can wait. POV's `union` stays `group`.
  - Cut faces take the cutter's surface if it has one, otherwise the
    enclosing `with-surface`. That matches POV.
- **Verification.** Render Texaco and compare it with `texaco.gif`.
  Also add a focused test scene: a cube minus a sphere, a sphere
  intersected with a cube, and a bowl viewed from inside and outside,
  including reflections and shadows.

**Done when:** the Texaco still is recognisably `texaco.gif` at
640×480, and (optionally) the 24-frame rotation plays back.

**Side quest:** `redball/red.pov` is a single metallic green sphere
in front of an `ambient 1` plane. It can be ported **today** and is
the cheapest way to check the camera conventions before CSG lands.

---

## 5. Xmastree (priority 2)

**Scene.** `xmastree/xmastree.pov`, final settings `gDetail = 4`,
`gAngle = 3`, 4:3 frame, `direction 2*z`.

**What it builds:**

- **Branches:** 7 layers × 10 branches of "bead chains" built along a
  path. Each bead is a small sphere placed with rotate, then a path
  translate, then a Gaussian jitter translate. That comes to
  **≈16,700 spheres**: per branch, 29 + 70 × stages beads, summed over
  layers of 0–6 stages.
- **Ornaments:** 280 candidate slots, ≈140 filled.
  - Four styles of ball ornament. Each is a CSG `difference` of spheres
    or cones or radial cylinders against a cap cylinder, plus a hook
    that is a CSG notched ring and a torus loop.
  - Materials: `P_Gold3`/`Red`/`P_Silver3`/`Yellow`/`Blue` with
    `F_MetalC`; hooks `T_Silver_3C`.
  - One style in 21 is a flat wooden-frame star from `trainorn.inc`
    (box ∪ rotated box, minus a cylinder, `yellow_wood` pigment).
    `hasTrain` is 0, so **the smokestack mesh is never instantiated
    here**, though the include still parses it.
- **Trunk:** a cylinder and a sphere, `T_Brass_3E`.
- **Base:** a CSG stand of cylinders and tori (a notched rim and nine
  torus cut-outs), `T_Wood25`.
- **Lights:**
  - A shadowless `Gray60` point light overhead.
  - A **spotlight that is also a 6×6 rectangular area light** (9×9
    samples), `White * 1.5`, aimed at the tree.
- **Backdrop:** a white ground plane, a white `sky_sphere`, and a
  hollow white sphere of radius 2000 around everything. All three
  collapse to a white `:background` plus the plane.
- A compass object, only drawn when `gDetail < 4`.

### Gaps (on top of Texaco's)

| # | Gap | Need | Size | Notes |
|---|---|---|---|---|
| X1 | **BVH builder** (`bvh` over a list of shapes) | Must | M | Already written up under "Future directions: real BVH". A median split on the longest centroid axis over `Bounded(Group)` nodes. Without it, every ray tests ~17k spheres. Apply it per branch or layer, then over the whole tree. |
| X2 | **Torus primitive** | Must | M | A quartic solver. Needs `hit_test`, `bounds`, and **CSG crossings**, because the base subtracts tori. Also used by cpot and coffeecup. The "More primitives" list already names it as next. |
| X3 | **SDL: seeded RNG** | Must | S | `(rng seed)` returning a value threaded through the code, or a stateful `(rand r)` host object; `rand-gauss`. Keep it deterministic so renders are repeatable. |
| X4 | **SDL: numeric and vector helpers** | Must | S | `floor`/`int`/`round`; `normalize`, `length`, `dot`, `cross` in the stdlib; `(affine-apply a p)` so bead centres can be **computed** instead of wrapped in three `Transform` nodes each. |
| X5 | **SDL: building large lists efficiently** | Should | S–M | There is no `concat`/`mapcat`/`into`. Building a 17k-element vector with repeated `conj` onto `Rc<Vec>` is O(n²). There may also be deep-clone costs when large subtrees are wrapped. Measure first, then add `concat`/`mapcat` (and possibly `for` as desugar sugar). |
| X6 | **Shadowless lights** | Should | S | A bool on `Light` that skips the shadow walk. Without it: drop the overhead fill light and raise ambient. |
| X7 | **Rectangular (quad) area light** | Should | S–M | Already listed as light-types Phase 6. A disk of radius ≈3.4 is a fine stand-in. |
| X8 | **Area light that is also a spot** | Should | M | `LightKind` is either `Spot` or `Area`. POV lets a light be both. Options: optional cone parameters on `Area`, or a combined variant. Stand-in: an area light alone, since the tree sits well inside the 20°/45° cone and only the cone edge on the far ground would differ. |
| X9 | **Procedural pigments: `wood` + `turbulence` + `color_map`** | Should | L | Needed for the frame ornament and the `T_Wood25` base. Requires **object-space texture coordinates**, because POV patterns move with the object. `RayHit` would need to carry a texture-space point captured through `Transform` nodes, and `Surface.color` would become a `Pigment` (solid, checker, wood…). Includes a Perlin-style noise function. Stand-in: flat colours from the middle of each colour map. |
| X10 | **Layered textures** (`T_Wood25` = two wood layers, the top partly clear) | Could | M | Only matters once X9 exists. One layer looks fine. |
| X11 | **Transform collapsing** | Could | S | Already listed under "Future directions". X4's computed centres avoid most of the cost here, but ornaments still stack transforms. |
| X12 | **POV-identical RNG** | Could | S | Only if exact ornament placement matters (§3.5). |

**Done when:** gAngle 3 at 640×480 renders in reasonable time and
looks like the original (the small `.xvpics/xmastree.bmp` thumbnail is
the only reference in the repo). Area-light penumbra should show up in
`render-samples.png`.

**Performance note.** The adaptive sampler plus an area light over
~17k beads is the heaviest scene this renderer will have seen. Use
`render-heatmap.png` before and after X1 as the measurement.

---

## 6. The remaining projects

Suggested order, from least new work to most.

| Order | Project | What's in it | New gaps beyond Texaco + xmastree |
|---|---|---|---|
| 3 | `redball/red.pov` | One metallic green sphere, `ambient 1` plane | None. Do it early as the calibration port (§4 side quest). |
| 4 | `braids/braids.pov`, `train/train.pov` | Same lights and camera as xmastree. Braids: 200×6×8 = 9,600 blue spheres in twisted rope braids. Train: just the compass. | None once X1 is done. `braids/box.pov` is a one-sphere test. |
| 5 | `ornament/orn.pov` | The wooden train-engine ornament: CSG boxes and cylinders in four `wood` + turbulence colours, a **3,312-triangle smokestack** (`smokestack.inc`, an AC3D export), and a tilted plane | **POV `triangle` mesh → OBJ** (S): a one-off conversion script (the file is plain `triangle { <a>,<b>,<c> }` lines). Then `load-obj` + `bvh`. Wood pigments (X9) matter more here: it's all painted wood. |
| 6 | `magic/nba.pov` | Three boxes in `T_Wood7/23/28` (one with `rgbt` 0.9 over wood), a checker ground, a white sky sphere, four lights | **Two-colour checker with scale** (S): the current checker uses unit cells, and the second colour is half the first. POV's `checker A, B scale s` needs both colours and a scale. Layered woods (X10). |
| 7 | `cpot/cpot.pov` + `coffeecup.inc` | Coffee pot and two cups: heavy CSG (differences, one intersection with a box), tori, `Chrome_Texture`, `Dark_Wood`, `T_Glass4` cups, a `phong` finish, a checker plane with `scale 5` | Specular power (T3), `phong`/`phong_size` mapping, two-colour checker. `T_Glass4` has **`ior`** in 3.x: without refraction the cups will look like tinted film rather than glass, which is acceptable for now. |
| 8 | `snowman/*` (`avatar.pov`, `sphere.pov`, `sphere2.pov`, `moldingtest.pov` + 6 includes) | A snowman blob, a "room" with a desk, window glass and blinds, clocks, outlets, a mirror, a yard height field, spotlit area lighting | Much more than the rest. See below. `snowman_workdir` duplicates `snowman_avatar` apart from xv thumbnails. |

### Snowman-only gaps

- **`blob`** (metaballs; L): an isosurface with root finding. Used for
  the snowman body. A stand-in is "the perfect body" that's already
  commented out in the file: a union of two spheres.
- **Normal perturbation** (L): `normal { bumps | wrinkles … turbulence }`
  on the snow, the walls, and `Brushed_Aluminum`. Needs the same
  object-space noise as X9 plus a normal-perturbation hook in the
  shading code.
- **`height_field`** from TGA/PNG (M) and **`image_map`** pigments (M):
  the yard and an experiment in `sphere.pov`. A height field can
  become a triangle mesh generated at load time.
- **`rgbf` filter transparency** (S–M): transmission tinted by the
  pigment colour. Colored transmission is already deferred in the
  transparency plan.
- **Refraction / `ior`** (L): glass panes, `FlatGlass`, the mirror
  glass. Already listed under "Future directions: refraction".
- **`brick` pattern with textures per brick and mortar** (M): the
  `Wood_Floor` texture. Needs X9 plus textures chosen by pattern.
- **`sky_sphere` with a `gradient` colour map** (S): a background
  function of ray direction.
- **Truncated cones** (S): the snowman's nose and hat. Generalise `Cone`
  to two radii.
- **`diffuse 1.5`, `phong_size 120`, `global_settings { ambient_light }`**
  (S): covered by T3 and the compat layer.
- **`intersection` with `plane`** (covered by T1 if planes act as
  half-spaces).
- **Light sources inside CSG objects** (the clocks): these already work
  because lights live in the scene graph.

A reasonable first snowman port uses stand-ins for blob, normals,
height field and refraction, and adds each feature afterwards.

---

## 7. Consolidated feature list

| Feature | Size | Texaco | Xmas | Other users | Section |
|---|---|:-:|:-:|---|---|
| CSG difference/intersection (+ plane half-space) | L | **Must** | **Must** | cpot, ornament, snowman | T1 |
| `_pov.lisp` compat layer (colours, box, starting-point surfaces) | S, growing | **Must** | **Must** | all | T2, §3 |
| Specular exponent (tuning knob, optional) | S | Could | Could | redball, cpot, snowman | T3 |
| SDL frame loop / `dotimes` / padded names | S | Could | — | future animation | T6 |
| BVH builder | M | — | **Must** | braids, ornament mesh | X1 |
| Torus | M | — | **Must** | cpot, coffeecup | X2 |
| SDL seeded RNG + Gaussian | S | — | **Must** | braids/train (seeded, unused) | X3 |
| SDL `floor`/`int`, vector math, `affine-apply` | S | — | **Must** | general | X4 |
| SDL `concat`/`mapcat`, list-building performance | S–M | — | Should | braids | X5 |
| Shadowless lights | S | — | Should | braids, train | X6 |
| Quad area light | S–M | — | Should | braids, train, snowman | X7 |
| Area light + spot cone | M | — | Should | braids, train, snowman | X8 |
| Object-space texture coords + `wood`/turbulence/`color_map` | L | — | Should | ornament, nba, cpot, snowman | X9 |
| Layered textures | M | — | Could | nba, snowman | X10 |
| Transform collapsing | S | — | Could | general | X11 |
| POV-identical RNG | S | — | Could | braids | X12 |
| POV triangle mesh → OBJ conversion script | S | — | — | ornament | §6 |
| Two-colour, scalable checker | S | — | — | nba, cpot, moldingtest | §6 |
| Truncated cone | S | — | — | snowman | §6 |
| `rgbf` filter (tinted transmission) | S–M | — | — | snowman | §6 |
| Gradient sky sphere | S | — | — | snowman | §6 |
| Normal perturbation (bumps, wrinkles) | L | — | — | snowman | §6 |
| Brick pattern (textures by pattern) | M | — | — | snowman | §6 |
| Height field / image map | M each | — | — | snowman | §6 |
| Blob / metaballs | L | — | — | snowman | §6 |
| Refraction / `ior` | L | — | — | cpot (cups), snowman | §6 |

---

## 8. Suggested work order

Each step either lands a port or unblocks the next one. Every renderer
change keeps existing scenes byte-identical by default, in the same
way as the earlier CLAUDE.md plans.

1. **CSG, phase 1.** The all-crossings query for sphere, cuboid,
   cylinder, cone and plane; the `Csg` node; `difference` and
   `intersection` in the SDL; a CSG test scene.
2. **Conventions check.** Start `scenes/_pov.lisp`. Port the xmastree
   compass (and optionally `redball`) to confirm handedness and camera
   zoom. This can happen alongside step 1.
3. **Port Texaco** and tune its look by eye. Add a specular-power knob
   only if the tuning needs it. Optionally add the animation frame loop.
4. *(Removed: POV surface-model emulation, per the decisions in §1.)*
5. **SDL groundwork for xmastree.** RNG and Gaussian, `floor`, vector
   math, `affine-apply`, `concat`/`mapcat`, and a list-building
   performance check.
6. **Torus**, including CSG crossings.
7. **BVH builder.**
8. **Port xmastree with stand-ins:** a disk area light, flat wood
   colours, and no shadowless fill light.
9. **Light polish:** shadowless, quad area, area + spot. Re-render the
   tree.
10. **Port braids and train.** They come nearly free after step 8.
11. **Mesh conversion script → port ornament.**
12. **Procedural pigments** (object-space coordinates, noise, wood,
    colour maps). Re-render ornament, xmastree and nba, and port cpot.
13. **Snowman**, in whatever order its features earn their keep:
    truncated cone and gradient sky first, then filter transparency,
    normals, blob, height field, refraction.

---

## 9. Open questions

- ~~How faithful should the ports be?~~ Answered: use this renderer's
  own shading and light model, tuned by eye (§1).
- **Exact random placement for xmastree and braids** (X12): worth
  copying POV's RNG, or is "same style, different arrangement" fine?
- **Where do ports live?** `scenes/pov/<project>.lisp` next to
  `_pov.lisp` would keep them apart from the renderer's own test scenes.
- **Animation:** is the rotating Texaco GIF part of "done", or is the
  still enough?
