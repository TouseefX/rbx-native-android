# Animation Graph behaviour measured in real Studio

Measured 2026-10-28 in Roblox Studio 0.745 (Vinegar, Linux) for rbx-native.
This covers what `docs/research/animation.md` §6 could only infer from the
docs. Over, Add and Subtract were already held to Studio by
`crates/rbx_animation/tests/studio_graph.rs` (to 1e-3, Cartesian Blend2D
on a square too).

## How

`crates/rbx_animation/tests/studio_graph_probe.rs` (an ignored test,
`RBX_GRAPH_OUT=<dir>`) writes `probe.rbxl` and `probe.lua`. The place holds
a rig with four Motor6Ds `P0..P3/J` and local KeyframeSequences:

- `T0..T11`, each moving one joint by 1 stud along one axis, so a blend's
  joint positions are its input weights;
- `R0..R4`, half-second ramps at x = 1..5 with y rising 0 → 1;
- one `AnimationGraphDefinition` per question, written by our own graph
  writer.

The Lua runs from the command bar. It registers every clip and graph with
`KeyframeSequenceProvider:RegisterKeyframeSequence` (local only, nothing
uploaded), plays each graph on the Animator, and steps it with
`Animator:StepAnimations(1/30)`. Each sample is printed as `GP|case|i|...`,
which ends up in Studio's log (`appdata/Roblox/logs/*_Studio_*.log`). The
whole run was done twice and gave identical output.

## Results

**Blend2D.** The weights are the gradient-band scheme already
implemented, with one change. Each input's weight is the minimum over the
other inputs of `1 − (p·v)/|v|²`, clamped at 0 but **not at 1**, then
normalised. In Polar mode the pair vectors are `((|b|−|a|)/mean, Δangle·2)`
(angles wrapped to (−π, π]; a point at the origin takes its partner's
angle). The ×2 is measured: on the irregular layout `(0,0) (0,1) (1.2,2)
(2.5,0.7) (−1,1.5)`, α = 2 fits to 5e-6 and α = 1 is off by up to 0.16.
The clamp is measured too: inputs at angles 0, 1, 2.5 with the target at
4.5 rad give 0.48766 / 0 / 0.51234 in Studio. Clamping at 1 would give
0.5 / 0 / 0.5. In all, 57 targets over six layouts match to 1e-5. Samples:

| layout | target | Studio weights |
|---|---|---|
| centre + compass at 1 | (π/4, 0.5) | 1/3, 1/3, 1/3, 0, 0 |
| centre + compass at 1 | (0.3, 0.2) | 0.67171, 0.16793, 0.16036, 0, 0 |
| compass at 1 and 2 | (π/4, 0) | 0.25 on each inner input |
| irregular (above) | (1, 0.4) | 0.47039, 0.29563, 0.21952, 0.01446, 0 |
| Cartesian, 5 irregular | (2, 2) | 0, 0.12752, 0.87248, 0, 0 |

**RandomSequence.** We recorded 40 picks for each of Seeds 0, 1, 2, 3,
7, 42, 1000 and 123456, over 2, 3 and 5 inputs, plus 1:2:3 weights at
Seed 7. For one Seed, every input count and weighting is consistent with a
single shared sequence `u₀, u₁, …` in [0, 1), where each pick is the
input whose share of the summed weights holds `u`. A brute-force search
over generators found the sequence: `std::mt19937(seed)`, `u = x / 2³²`.
It matches all 320 picks. Other generators scored no better than chance:
Luau's `Random` (PCG32), PCG32 with other seedings, MSVC `rand`, minstd,
splitmix64, and the seed's float bits. Further facts:

- Seed 7.5 picks as 8 and −2 as 4294967294.
- −1 differs between runs.
- Renaming the node changes nothing.
- The same input can play twice in a row.
- The first pick is made when the track starts. Each later pick comes on
  the step where the previous input finishes.

Seed 7, 3 inputs: `0020122120101001001122201102022201122002`.

**Transitions.** CubicV2 over 1 s gives 0.00015, 0.00119, 0.004, 0.00948,
… 0.5 at 1/30 s steps, which is `4t³` up to the midpoint (cubic in-out).
Linear is linear. Interrupting an A→B blend at 0.5 with C makes A drop out
at once, and B fades into C over the full duration. There is no frozen
snapshot.

**Select / Priority Select.** With a Selection naming no pin, the empty
default included, nothing plays: the joints sit at rest, and Studio logs
"Selection 'Nope' is not an input pin name". Changing an active Select to
such a name fades the current input out to rest over the default
transition. A Priority Select whose Triggers are all off plays nothing.

**Mask.** With a mask listing only `P0` at 0.5, P0 plays at 0.5, and P1,
which the mask does not list, plays at 0.

**Trim.** TrimStart 0.2 / TrimEnd 0 holds the pose at 0 s. TrimStart 0.3
/ TrimEnd 0.1 holds it at 0.1 s. A backwards trim freezes at TrimEnd.

**One Shot.** Re-triggering a shot while it plays restarts it from 0.

## Instance attributes in files

We loaded the Studio-saved `tests/fixtures/studio_state_machine.rbxm` into
an XML place, using `game:GetObjects("rbxasset://…")` from a copy in
Studio's content folder (deleted afterwards). Ctrl+S then saved the place
locally. `GetAttribute("EntryState")` returns the `State` ObjectValue.

- In a binary file the attribute sits in `AttributesSerialize` as type
  `0x25` + `u32` referent, as before.
- In XML, Studio leaves it out of the blob
  (`AttributesSerialize` = `AAAAAA==`, zero entries). It writes it as
  `<Ref name="__attrRef_EntryState">RBX…</Ref>` straight after the blob.

The graph subtree of that save is
`tests/fixtures/studio_state_machine.rbxmx`. `rbx_xml` now folds
`__attrRef_*` into the blob on reading and splits it back out on writing.

## Still not measured (UNVERIFIED in the code)

- Whether a trigger parameter lasts exactly one step.
- How events split across a transition.
- Whether an "(Any)" transition can re-enter the playing state, and which
  generator breaks ties between transitions.
- What a Priority Select does when every Trigger turns off after one fired,
  and whether Interruptible guards moves to lower inputs.
- What a Mask naming no shared mask does.
- Blend phase-sync beyond the documented virtual duration.

None of these was in scope for this pass. Each needs its own probe.
