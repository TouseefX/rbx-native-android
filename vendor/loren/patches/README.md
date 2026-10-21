# Performance patches to Loren's runtime

Each `.patch` here is a self-contained diff against the pinned upstream
commit (`650154a`, `v2.0.0-beta.2`), with paths as they are in fabi's
repository (`cli/project/loren/...`), so it can be offered upstream as is:
`git apply vendor/loren/patches/<name>.patch` in a Loren checkout. The
vendored runtime in `../project` already has every patch below applied.

Rules each patch was held to:

- Measured before and after with `cargo run --release -p rbx_loren --example
  loren_bench` (plus a narrower probe where the bench case is a mix), runs
  interleaved, on a shared machine; only differences larger than the
  run-to-run spread count.
- Behaviour identical: same wire bytes, errors and ordering. Loren's own
  unit specs (`vendor/loren/test/specs/unit`, run by `cargo test -p
  rbx_loren --test loren_specs`) pass with the patch, including the
  differential checks against the frozen v2 codec.
- The numbers come from rbx-native's Luau sandbox, which runs Luau's
  interpreter without native code generation. Roblox servers run Loren's
  `@native` functions compiled, so the gain there has to be measured again
  (Loren's own bench place, `test-project/bench.project.json`).

## 0001: Codec, inline one-byte string lengths

`0001-codec-inline-one-byte-string-lengths.patch`

A string shorter than 128 bytes (nearly every string a game sends, and
every map key) has a one-byte length. `writeStr` now writes that byte
itself instead of calling `putVarint`, and the decoder reads it inline for
string values and map keys, calling `readVarint` only for longer lengths
and for every malformed case (an overrun or a bad varint still goes through
exactly the old checks and errors).

| Probe (us per op, best of 5, 3 interleaved runs) | Before | After |
| :--- | ---: | ---: |
| `writeArgs` of one 5-byte string | 0.91 - 0.95 | 0.87 - 0.89 |
| `writeArgs` of a 2-key map | 4.17 - 4.26 | 3.97 - 4.03 |
| `writeArgs` of the 8 mixed values below | 8.21 - 8.39 | 7.93 - 8.08 |
| `readArgs` of those 8 values | 4.36 | 4.03 - 4.14 |
| bench `codec_untyped` (median, us per 1000) | 15021 - 15358 | 14525 - 15190 |

(The 8 values: 6, 42, -1.5, "hello", true, a Vector3 and `{ tag = "hit",
n = 3 }`.) So about 5-7 % on string and map encode/decode, about 3 % on
that whole mixed message.

## Tried and not kept

Measured with the same probes, and reverted because the difference stayed
inside the run-to-run spread:

- Skipping the cycle-detection mark (`visiting[v]`) for tables with no
  table values (`writeValue`): 4.16 -> 4.13 us on a 2-key map.
- Merging the boolean and nil branches of `writeValue` and inlining
  `reserve`: 1.55 us before and after.
- `writeArgs` walking its varargs with `select(i, ...)`: a packed table is
  faster in the interpreter from 3 arguments up (0.64 -> 0.50 us at 3, 1.42
  -> 0.99 us at 8), but it allocates per message, and under native code
  `select` is a fast builtin, so it was not changed.

The send paths (`ServerSignal` fire closures, `ClientNet.eventFire`) and the
receive loops (`Transport`, `Signal` runners) are already specialised per
route in 2.0.0-beta.2; no change there beat the noise.
