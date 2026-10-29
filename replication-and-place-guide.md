# Team Create replication, place reconstruction, and live editing

**A research and implementation guide for the receiving client and a prospective Rust server**  
**Evidence reviewed:** 29 October 2026  
**Scope:** collaborative Studio edit sessions, not playtest/gameplay.

---

## Read this first

Your client joining a Team Create server does not, by itself, give you an `.rbxl` file. The server can send a **schema**, an **initial instance snapshot**, incremental **instance/property changes**, and separate **script-editor traffic**. A client must decode and apply those messages to an in-memory data model, then serialize the reconstructed state as a new `.rbxl`. The result is only as complete as the state the server actually sent and the decoder understood.

This reference goes beyond the earlier short guide. It describes the observed game-receiver layouts down to individual fields, shows two complete annotated synthetic records and a property update, contains a runnable Rust wire-to-place example, and lays out a client and server design including script concurrency. It does **not** claim that those game-session bytes are the native Team Create protocol.

### Evidence labels used throughout

| Label | Meaning |
|---|---|
| **[GAME]** | Observed or implemented by the public Boblox **game-session** receiver at commit `8c46b6dc175e22f50a7661bd19ff86eb5dbe4287`. Useful as a concrete reference; not proof of Studio Team Create compatibility. |
| **[STUDIO STATIC]** | Found by inspecting a particular official Studio binary, without running a Studio session. Names, isolated machine-code behavior, or local data structures do not by themselves establish the network envelope. |
| **[DESIGN]** | An implementation recommendation or an invented example. It is not a native Roblox packet. |
| **[UNKNOWN]** | Required for the real target but not established by available evidence. |

The game receiver, the particular Studio executable, and your Rust port are **not known to be the same build or protocol**. You said your port can join Team Create; its source and a Team Create capture were not supplied. Therefore there is no verified native-Team-Create decoder or server implementation in this document. Where the native details are unknown, the guide says what evidence is missing rather than inventing bytes.

### Quick answers

1. **Where is the place?** Usually in replicated state, not necessarily a downloadable original `.rbxl`. Gather initial records, resolve parents and references, process relevant changes and script source, then write a new file.
2. **Where do I hook my Rust Boblox port?** At the callback that receives a **complete, decrypted, already transport-reassembled logical replication message**, before a game runtime discards or simplifies it. Which callback that is depends on your port.
3. **How do I know the IDs and types?** In Boblox's observed game format, the session schema supplies ordered class/member/event tables. Snapshot property slots are class-local; live property IDs are global and schema-specific.
4. **How do I synchronize later edits?** Apply creation, property changes, parenting, deletion and editor operations in protocol order, with a consistent initial snapshot and stable object identities. The corresponding **Team Create wire formats are not all known**.
5. **How do I edit scripts?** Treat editor documents and their concurrent operations as a separate subsystem; `Script.Source`, compiled game bytecode and an editor buffer are not interchangeable. Some nested Studio operation data is statically understood, but its event envelope and complete semantics are not.
6. **Can this already be made to work with unmodified Studio?** Not reliably from the available evidence alone. The missing native bootstrap, schema/tail, instance lifecycle and script event/operation details are listed in §18.

### Contents

1. [Whole-system map](#1-whole-system-map)
2. [Start in your existing client](#2-start-in-your-existing-client)
3. [Binary primitives and framing](#3-binary-primitives-and-framing)
4. [Schema message and tables](#4-schema-message-and-tables)
5. [Value formats](#5-value-formats)
6. [Initial instance batches](#6-initial-instance-batches)
7. [Record-by-record worked example](#7-record-by-record-worked-example)
8. [Live instance and event messages](#8-live-instance-and-event-messages)
9. [Build an instance tree](#9-build-an-instance-tree)
10. [Scripts and live document operations](#10-scripts-and-live-document-operations)
11. [Design the collaboration server](#11-design-the-collaboration-server)
12. [Loading and saving place files](#12-loading-and-saving-place-files)
13. [Runnable Rust example: records to `.rbxl`](#13-runnable-rust-example-records-to-rbxl)
14. [A practical integration plan for your port](#14-a-practical-integration-plan-for-your-port)
15. [Verification and test matrix](#15-verification-and-test-matrix)
16. [Troubleshooting](#16-troubleshooting)
17. [Security and resource limits](#17-security-and-resource-limits)
18. [What remains unknown for native Team Create](#18-what-remains-unknown-for-native-team-create)
19. [Sources and provenance](#19-sources-and-provenance)

---

## 1. Whole-system map

### 1.1 Receiving and reconstructing a place

```text
connection / authentication / transport                [your existing join path]
    ↓  reassembly, reliability, any required decryption
logical replication message                              [hook here]
    ↓  message and item dispatch
schema + typed records + incremental changes             [decode]
    ↓
reference map + properties + parent graph + assets       [apply]
    ↓
editor document source and operations                     [separate; not yet decoded]
    ↓
in-memory DataModel at a known synchronization point
    ↓
.rbxl serializer → reconstructed place file
```

The diagram has a single conceptual direction; real sessions can interleave chunks and live changes, and the exact ordering for native Team Create is **unknown**. A successful parser is only one layer. For a usable result you also need correct application semantics, resolved references, complete source data, and a consistency point.

### 1.2 Serving unmodified Studio

```text
local .rbxl / .rbxlx
    ↓ file decoder
persistent authoritative document + instance tree
    ↓ build-/session-specific schema, object identity, filtering
snapshot encoder → native transport → Studio clients
    ↑                                      ↓
change validator ← native event decoder ← client edits
    ↓
ordered state mutation → broadcast to other clients → durable save
```

**[DESIGN]** This is the shape of a server, not a working native packet sequence. The file decoder, data model, persistence, and internal collaboration design can be implemented now. The Team Create adapter cannot honestly be completed until its missing native wire details are verified.

### 1.3 Four different identities and formats

Do not confuse:

* **On-disk format:** `.rbxl`/`.rbxlx` and their internal referents. A place file is not a dump of replication messages.
* **Network identity:** the reference fields used by a particular connection and protocol; lifetimes and namespaces may be session-specific.
* **In-memory identity:** a `WeakDom` referent or your own object key. It is not automatically a network ID.
* **Schema identity:** class and property indices whose meaning depends on the exact schema, not permanent Roblox class IDs.

A safe design keeps explicit maps between these domains. Two objects can have the same name, and paths change when a parent/name changes; neither names nor paths are suitable stable identities.

### 1.4 What exists and what does not

| Component | Status |
|---|---|
| Read/write local `.rbxl` | Available with Rust place/DOM libraries; the small example in §13 was run and reopened. |
| Parse the public Boblox game schema and certain game records | Detailed below; the separate research project contains a bounded partial schema reader. |
| Generate a few offline game-style records | Previous experiment parsed with Boblox; not verified as accepted by Studio, and its trailer is unverified. |
| Pinned Studio Live Scripting data | Some signal names, JSON fields and one narrow transform helper were statically examined. |
| Join and authenticate an unmodified Studio client against a new Rust server | Not implemented or tested here. Your own claimed join path was not inspected. |
| Complete Team Create snapshot, live edits, script source and two-way editing | Not established or tested. |

---

## 2. Start in your existing client

### 2.1 Find the correct input boundary

A raw network packet is not necessarily a replication message. It may contain a fragment, multiple framed messages, a transport header, or encrypted data. Instrument the point **after your client has authenticated, reassembled ordered data, decrypted it if applicable, and determined one logical message boundary**.

For an authorized test session, log metadata such as:

```text
session-epoch, channel, receive-order, logical-message-id, message-length,
schema-fingerprint-if-known, parse-result, item-count, instance-count
```

Log a hex prefix only if it is safe for your data and necessary to investigate. Do not log or share credentials, tickets, session keys, private source code, place data, or user identifiers. A valid join response alone does not show that the receiver is handling the replication stream.

**[GAME]** In the pinned public Boblox source, useful starting points are:

| Function/path | Role |
|---|---|
| `client/src/engine/live_runtime.cpp` | Dispatches logical message IDs and applies part of the game receiver state. |
| `solve/repl_snapshot.cpp`: `parseJoinOuter`, `parseJoinSchema` | Inflate and parse the observed `0x97` schema prefix. |
| Same file: `parseDeltaBatch`, `parseJoinRecords` | Inflate a game-style batch and decode snapshot records. The batch parser does **not** validate the unexplained trailer. |
| `solve/repl_stream.cpp`: `walk83ItemsEx`, `parseSpawnItem`, `parseEventItem`, `propTagsForSchema` | Walk a game-style `0x83` item chain and decode certain live items using schema tables. |
| `solve/repl_core.cpp`: `read_ref`, `read_tagged_value` | Decode reference and value extents. |
| `solve/joindata.cpp` and the runtime application path | Convert some wire values and apply game records; useful to study separately from pure parsing. |

Your Rust conversion may have renamed or omitted these functions. In particular, a join-only port can connect but never call the world-replication path.

### 2.2 Exact entry points in the public Boblox reference

**[GAME]** If your port retains the public structure, `client/src/engine/live_runtime.h` distinguishes two inputs:

* `LiveRuntime::feed(bytes)` accepts already-delivered application stream bytes and consumes its **compact-varint-framed** messages as complete frames arrive. Its comment describes the RbxTransport application/channel path; it is not a raw encrypted UDP-packet interface.
* `LiveRuntime::feedMessage(message)` accepts one complete RakNet application message beginning with the logical message ID. In the C++ reference it **internally prepends** a compact frame length and calls `feed`. If your port copies this behavior, do not prepend that length a second time before calling the complete-message method.

The internal `handle` dispatches `0x97`, `0x84`, `0x83`, `0x81` and `0x8d`. It can queue a limited number of pre-schema `0x83` messages, accumulates snapshot records, and exposes diagnostic methods such as `has_world`, `root`, `snapshot_records`, `live_messages` and `stats`. `root` is null until the game runtime finalizes a snapshot. During finalization that implementation clears its staged record vector after applying it; capture typed records before that point if you need to inspect their original identities and raw values.

Do not assume the game runtime's resulting tree is a faithful Team Create place: it applies only its implemented paths, skips NetAsset batches, does not apply the parsed `0x01` as deletion, simplifies some identities, and may run game script data. For an extraction tool, use a non-executing data-model path and inspect decoded records/coverage rather than executing untrusted content. Your Rust port may differ or lack these methods entirely.

### 2.3 A safe receiver state machine

**[DESIGN — pseudocode, not native packet code]**

```text
on_logical_message(session, message):
    record_metadata_without_secrets(message)
    if verified_decoder_does_not_match(session.build, message):
        preserve_diagnostic_and_report_unsupported()
        return
    decoded = decoder.decode(message, session.schema, session.protocol_state)
    for change in decoded.changes_in_order:
        stage_or_apply(change, session.epoch)
    update_consistency_and_completeness_status()
```

Keep per-session state: transport epoch, schema fingerprint and tables, expected IDs, received shared values, snapshot staging, pending references, pending changes, and unsupported features. Never silently fall back from a failed Team Create decode to game assumptions and call the result complete.

If messages arrive before the schema, retain them within bounded limits only if the transport/state makes that safe; otherwise fail and request/retry synchronization. If a parser cannot establish a value's length, it cannot reliably skip that value or find the next record.

---

## 3. Binary primitives and framing

### 3.1 Endianness, variable integers, and lengths

**[GAME]** The referenced Boblox snapshot/stream reader uses:

* `u16`, `u32` and `u64` fields in the layouts below: little-endian unless stated otherwise.
* `varint`: unsigned LEB128. For example, `300` is `ac 02`. Each byte carries seven value bits and a continuation bit.
* Zigzag for selected signed values: `signed = (unsigned >> 1) ^ -(unsigned & 1)`. Thus encoded unsigned `1` represents `-1`.
* A `string` in the schema is `[varint byte length][that many bytes]`. It is not NUL-terminated.
* Typical `f32` components occupy four bytes; `1.0` is `00 00 80 3f` in little-endian IEEE-754.
* Every count, addition, multiplication, integer conversion and allocation must be checked. Reject truncated and overflowing input.

**Important:** the application framing integer used by one Boblox stream path is a **different format** from the inner LEB128. That `compactVarint` uses the first byte's top two bits to select a 1-, 2-, 4- or 8-byte big-endian-form value; the remaining six first-byte bits are data. The RakNet and RbxTransport paths do not have to expose identical framing boundaries. Do not insert or strip that prefix on your connection just because an example uses it; establish the boundary in your own port first. Likewise, a byte `0x83` at a transport layer is not automatically the logical replication message ID `0x83`.

### 3.2 Game reference encoding

**[GAME]** For an instance/parent reference in the studied parser:

```text
reference = [unsigned LEB128 lo]
            [u32 little-endian id, only if lo != 0]

lo == 0  → a null/sentinel reference; no four-byte id follows
lo != 0  → a non-null pair (lo, id)
```

The `lo` values' full meaning, lifecycle, and relationship to Team Create identities are **unknown**. The pair is a useful provisional game-record key, not proof that it is a globally stable GUID. Some other Boblox GUID readers have a different sentinel rule, so do not assume every similarly named reference uses the same layout.

The Boblox game runtime sometimes uses only the 32-bit `id` in its world-application paths. That convenience is not evidence that discarding `lo` is safe for your sessions. Preserve the full observed pair and a connection epoch until its semantics are verified.

---

## 4. Schema message and tables

### 4.1 Why the schema matters

The schema is a runtime dictionary. The decoder uses it to answer:

* Which class does class index `358` name in this exact session?
* Which property occupies member slot `2` for that class?
* What tag determines the length and decoding of that property's value?
* Which global property ID or event ID maps to which member or event?
* Which event arguments and shared names exist?

A schema from a different version can produce plausible-looking but wrong names and lengths. Fingerprint and retain the original schema message with your session logs; do not persist numeric IDs as globally meaningful without the corresponding schema. A hash is a diagnostic fingerprint, not proof two schemas have the same semantics.

### 4.2 Outer `0x97` message

**[GAME — unframed logical message]**

```text
offset  size  meaning
0       1     97
1       2     two observed build/magic bytes
3       2     00 00
5       4     decompressed byte length, u32 LE
9       ...   one Zstandard frame, to the end of this logical message
```

The public newer game sample has magic `50 d3`; an older sample has `a3 cf`. They are identifying observations, not proof that those bytes are a version number or that Studio uses them. Verify a standard Zstandard frame, cap compressed and decompressed sizes and decoder window, require the exact expected decompressed length, and reject unexpected trailing data for this observed outer shape.

### 4.3 Decompressed schema prefix

**[GAME]** `varint` below means unsigned LEB128; string means the length-prefixed byte string above.

```text
varint schema_version             # observed 3; encoded as one byte there
u64 LE schema_flags               # observed 15
varint enum_count
  repeat enum_count:
    string enum_name
    u8 enum_marker                # observed 07
varint class_count
varint property_bound             # table/preallocation bound, not a property itself
varint event_bound
  repeat class_count:
    string class_name
    varint member_count
      repeat member_count:
        string member_name
        u8 value_tag
        if value_tag == 7:  u16 LE extra    # enum table metadata
        if value_tag == 60: varint extra    # descriptor metadata
    u16 LE class_flags
    varint event_count
      repeat event_count:
        string event_name
        u8 parameter_count
        repeat parameter_count:
          u8 argument_tag
          u8 argument_extra_low
          u8 argument_extra_high
varint url_prefix_count
  repeat: string prefix
varint shared_name_count
  repeat: string name
opaque bytes to the end of the decompressed schema
```

This is the understood prefix of the public game schema, not a complete schema generator. The tail is retained but not decoded by the research reader. It may matter to native clients. The per-parameter two extra bytes are retained as metadata; do not invent their meaning for every tag. Decode under explicit limits and fail if a version/flag/layout is unsupported.

The newer public sample had **606 enums, 636 classes, 26 URL prefixes, 127 shared names, property/event bounds 10,301/7,003, and 30,640 opaque tail bytes**. The older public sample had **602 enums, 630 classes, and bounds 10,145/6,937**. These are sample observations, not universal constants or a Team Create schema.

### 4.4 Class-local slots versus global IDs

**[GAME]** A snapshot property or spawn property refers to an **absolute zero-based slot in that class's ordered member list**. Even though the C++ variable is called `delta`, Boblox's parser uses it directly as a slot; do not accumulate it as a relative delta. Byte `ff` ends the property loop, so that byte is not a usable slot in this encoding.

A `0x03` live property update instead carries a **global `u16` property ID**. In the studied game receiver the base mapping is built by concatenating the ordered member lists for all classes:

```text
global_property_id = number of members in all earlier schema classes
                   + member's class-local slot
```

There is also a sentinel at the member-count bound, and Boblox applies a set of **per-class/property wire-shape overrides** after constructing the table. A correct port needs both the current schema and the applicable override rules. The table is not deduplicated by property name; inherited-looking member names can occur for multiple classes. A global event ID is found by concatenating the ordered class event lists, then looking up its argument descriptors. Do not confuse these IDs with a snapshot's one-byte member slot.

**[UNKNOWN]** None of this proves Team Create uses the same global ID construction, overrides, property filtering, or schema tail. Never hardcode a game sample's IDs as a native Studio API.

### 4.5 Inspect a schema instead of guessing its IDs

If you still have the earlier `roblox-team-create-rust` research project, its optional offline `schema-info` command can list a class's slots and tags from one **unframed, plaintext** schema message. For a reproducible **game-only** sample, from a directory containing the research project:

```sh
git clone https://gitlab.com/kingdudely/Boblox.git boblox-reference
git -C boblox-reference checkout 8c46b6dc175e22f50a7661bd19ff86eb5dbe4287
cd roblox-team-create-rust
cargo run --locked --features candidate-snapshot -- schema-info ../boblox-reference/run/join97_new_frame.bin Part
```

Selected output from that public game sample is:

```text
magic=50d3 version=3 flags=15
Class Part index=358 ...
  [2] Anchored tag=9 extra=None
  [12] CFrame tag=27 extra=None
  [59] Name tag=49 extra=None
```

The command prints additional counts and all member slots; it is an offline inspector, not a server. Its Rust parser keeps the schema **prefix** and the opaque tail length but does not expose every event descriptor needed by a live event decoder. Its version/flags/layout restrictions and limits can reject another build. If your authorized Team Create client actually receives a compatible schema, use that session's unframed message rather than this public game capture—but a successful prefix parse alone does not establish Team Create compatibility or validate the tail. This step is optional and does not require you to adopt the earlier large project; the standalone example in §13 needs only two crates.

### 4.6 Names and two different kinds of shared data

The `0x97` shared-**name** table is an ordered list used by the game tag-49 Name format. It is separate from the keyed shared-**string** entries in a `0x83/0x16` flag-1 batch. Both may be needed to reconstruct values; do not merge them into one table.

---

## 5. Value formats

These are **[GAME] reader shapes** from the pinned Boblox source, not a universal Roblox type specification. A tag determines bytes consumed; the class/member context and applicable override determine the actual semantic type. Some details below describe extents rather than a fully resolved value.

| Tag(s) | Boblox reader consumes / interprets | Notes |
|---|---|---|
| `2`, `8` | `varint length` + bytes | String/binary-string family; preserve raw bytes if not text. |
| `3` | zero bytes | Zero-width value. |
| `6`, `46`, `47` | 16 bytes | Context matters. The game `Script.Source` example using tag 47 is opaque, not editable source. |
| `7`, `34` | varint | Enum/integer-family wire extent; consult the schema and semantic mapper. |
| `10`, `44` | varint | Boblox's semantic reader applies zigzag to the examined signed values, including tag 44. |
| `9` | one byte | Boolean/u8 family. The runnable demo deliberately accepts only 0/1. |
| `11`, `16`, `17` | 4 bytes | Depending on context, `f32` or `u32`-shaped. |
| `12`, `48`, `55`, `62` | 8 bytes | Context-specific; e.g. tag 55 can be Capabilities, not automatically `f64`. |
| `13`, `21`, `37` | 8 bytes | Includes UDim, Vector2, NumberRange-shaped values; component interpretation differs. |
| `14`, `40`, `67` | 16 bytes | Includes UDim2/Rect and another mixed layout; not necessarily four floats. |
| `15` | 24 bytes | Ray-shaped two Vector3 values. |
| `18` | 2 bytes | BrickColor-shaped integer. |
| `19`, `22`, `36` | 12 bytes | Three `f32`-shaped components, e.g. Color3/Vector3 in relevant contexts. |
| `20` | 3 bytes | Three `u8`-shaped components. |
| `24` | 4 bytes | Two 16-bit components. |
| `25` | 6 bytes | Three 16-bit components. |
| `26` | 12-byte position, `u8 kind`, optional 36-byte matrix if kind is zero | A CFrame-A shape in the source; not observed in the particular old schema and not a Team Create claim. |
| `27` | 12-byte position, `u8 kind`, optional 6-byte compressed rotation if kind is zero | CFrame-B; nonzero kinds use orientation lookup logic. Keeping 19 bytes is not the same as correctly reconstructing rotation. |
| `28`, `69` | reference as in §3.2 | Must resolve the target after all relevant objects arrive. |
| `29`–`32` | count and recursively tagged elements | See §5.3. Earlier comments in the public project saying these are not decoded are superseded by the later reader. |
| `33` | Asset/content string in context-dependent plain or prefix mode | See §5.4. |
| `35` | `u32 N`, then `N × 12` bytes | Boblox limits `N` to 20; the components' precise semantics still require context. |
| `38` | `u32 N`, then `N × 20` bytes | Sequence-shaped; Boblox limits `N` to 20. |
| `39` | 20 bytes | Keypoint-shaped. |
| `41` | `u8 flags`; if bit 0 is set, 20 bytes and, under the reader's extra condition, 4 more | Physical-properties-shaped; do not assume a fixed extent. |
| `49` | literal or shared Name | Details below. |
| `51` | kind byte, optionally compressed rotation and position | Optional CFrame-shaped. |
| `52` | 16 bytes | Context-specific. |
| `54` | `u16`, `u8`, then two `u32 length` + byte strings | FontFace-shaped. |
| `56` | `varint compressed_len`, `varint decoded_len`, compressed bytes | A bulk blob; decoding/semantics depend on context. |
| `58` | 7-byte header, `u8 N`, `N` bytes | Boblox bounds `N`; possible in-memory Huffman handling is separate. |
| `60` | `varint schema index`; unless index is `0xffff`, then 8 bytes and another varint | NetAsset-ID-shaped; full semantics remain limited. |
| `61` | varint form, then form-specific content | Empty, asset ID, or reference forms handled; other forms can fail. |
| `65` | 13 bytes | `u8 + f32 + f64`-shaped. |
| `66` | 3 bytes | `u8 + u16`-shaped. |
| `68` | four varints | User-shaped data. |

This is not a claim that every listed tag is correctly converted into an `.rbxl` value. Unknown tags, unverified value forms, client-specific overrides and unknown-length items must fail closed, with the containing raw message retained locally if appropriate.

### 5.1 Name, tag 49

In the studied reader the first byte `n` selects the shape:

* High bit clear and `n < 127`: `n` literal following bytes.
* `n == 127`: an additional varint length, then that many literal bytes.
* High bit set: low 7 bits select a shared name; the extended shared-name case with low bits `127` is rejected by this reader because it was not established.

For the worked example, `04 52 6f 6f 6d` is the literal Name `Room`. The shared-name table comes from the schema, not the flag-1 keyed shared-string batch.

### 5.2 References and delayed resolution

A property value can reference an object that has not arrived yet. Decode and store its observed reference key first; after allocating target objects, resolve it to the local DOM's reference type. Report unresolved references. Setting an unknown target to null makes a syntactically valid file while silently changing the place.

### 5.3 Tuple and map values

In the newer Boblox reader, tuple-family tags 29–32 are **self-describing**: a varint count followed by elements with their own tag and corresponding value; map/table variants include length-prefixed string keys. An element tag 0 can be empty; tag 7 carries extra enum metadata. Boblox imposes nesting/count constraints. The exact allowed arguments and Team Create use are unknown. Preserve ordering where it matters; a generic JSON conversion may lose type information.

### 5.4 Prefix-mode differences

For tag 33 and related tag-61 content, the game receiver chooses different wire shapes depending on the calling context. A snapshot property and spawn loop B use the non-prefix form; an event and spawn loop A can use prefix mode. Prefix mode can contain a prefix selector and an ID or string suffix. Do not assume that a value's schema tag alone determines its entire wire shape without the context and property overrides.

---

## 6. Initial instance batches

### 6.1 Outer batch

**[GAME — one logical item inside `0x83`]**

```text
83                      logical replication-chain message
16                      batch item type
flags                   one byte
compressed_len          u32 LE
decoded_len             u32 LE
zstd_frame              exactly compressed_len bytes
trailer                 additional bytes; full semantics unresolved
```

The exact frame and chain boundary are supplied by the transport/replication layer. Validate lengths before allocation, enforce a decompressed limit/window, and require the decoded byte count to match `decoded_len`. Do **not** split raw input whenever byte `16` or `83` appears; those bytes can occur within compressed or property data.

In the nine public game examples surveyed, flags `2`, `3`, and `4` were read as instance records; `1` is shared-string data, and `5` is NetAsset-shaped data. Their precise native meanings and Studio Team Create equivalents are not established.

### 6.2 Trailer: a concrete warning

In eight of those nine game examples, the trailer began with a little-endian number matching the decoded record count followed by zeros. In `frame024`, however, the first number is `691` while the batch has `903` decoded records; an ensuing varint is `212`, and the numbers happen to add to 903. That observation does **not** prove the extension's meaning. Boblox's `parseDeltaBatch` inflates the compressed region but **does not validate the rest of the trailer**.

The earlier offline encoder used a candidate `[u32 record_count][00][00]` suffix. Boblox accepting the compressed records did not validate that suffix. Do not use it as a verified Studio or Team Create trailer.

### 6.3 Shared strings and NetAsset batches

**[GAME]** Boblox parses flag-1 decoded data as repeated entries:

```text
16-byte key | u8 marker (0 or 1 in this reader) | varint length | value bytes
```

Store them by the full key. Exact use, lifecycle and Team Create behavior are unknown. Do not interpret their bytes as names or source without the appropriate consumer.

Flag-5 data has NetAsset-ID-shaped entries with a length-delimited payload. The public runtime skips this batch in its ordinary world application; its separate parser can frame entries but does not turn every asset into an editable instance. Assets, terrain and external content cannot be assumed present merely because the instance tree parsed.

### 6.4 The Studio builders that were actually seen

**[STUDIO STATIC]** In the pinned Windows Studio build, two inspected builders emitted `83 16` followed by kind `01` or `05`, little-endian compressed/uncompressed lengths, compressed bytes and a conditional zero. The observations identify those particular builders, not a complete instance snapshot or an actual Team Create session. In the **game** receiver, kinds 1 and 5 have the special meanings above. It would be a mistake to infer that those builders alone serialize all instances, or that unobserved Studio kinds 2/3/4 are confirmed.

---

## 7. Record-by-record worked example

This is **[DESIGN + GAME]**: the identities and names below are invented. Their class/member bindings were taken from the specific public game schema with magic `50 d3`, and the two records were independently accepted by that commit's Boblox schema/record parser. They are **not captured Team Create records**, not a complete `0x16` batch, and not safe to replay against Studio.

For this one schema:

```text
class 224 = Folder    slot 2 = Archivable (tag 9), slot 6 = Name (tag 49)
class 358 = Part      slot 2 = Anchored   (tag 9), slot 59 = Name (tag 49)
```

Another schema can assign different numbers or formats.

### 7.1 Instance record shape

**[GAME]** After inflating a record batch:

```text
reference of this instance
u16 LE class_index
repeat:
    u8 absolute class-member slot, or ff to end the loop
    typed value whose shape comes from the class schema and context
reference of parent
```

A compressed batch can contain several records concatenated. There is no general per-record byte length in this shape: the reader must know each property's value extent to find the next field.

### 7.2 Folder `Room`

```text
01 64 00 00 00      self reference: lo=1, id=100
 e0 00              class index 224: Folder
 02 01              slot 2 Archivable, tag 9: true
 06 04 52 6f 6f 6d  slot 6 Name, tag 49: literal 4-byte "Room"
 ff                 end of property loop
 00                 parent sentinel; no u32 follows
```

The record as one line is:

```text
01 64 00 00 00 e0 00 02 01 06 04 52 6f 6f 6d ff 00
```

A null parent does not universally mean `Workspace`. Boblox's game world builder can graft such a record to a root; the example in §13 grafts it to its own local `DataModel` root. How Team Create maps service roots or null parents is unverified.

### 7.3 Part `Block` under `Room`

```text
01 65 00 00 00         self reference: lo=1, id=101
66 01                  class index 358: Part
02 01                  slot 2 Anchored, tag 9: true
3b 05 42 6c 6f 63 6b   slot 59 Name, tag 49: literal 5-byte "Block"
ff                     end of property loop
01 64 00 00 00         parent reference: lo=1, id=100
```

One line:

```text
01 65 00 00 00 66 01 02 01 3b 05 42 6c 6f 63 6b ff 01 64 00 00 00
```

The in-memory graph is `DataModel → Room: Folder → Block: Part`. This is a minimal demonstration, not a full place with Workspace, services, terrain, scripts and all required defaults.

### 7.4 Later property update

For this exact schema, the game receiver's derived global ID for `Part.Anchored` is **5523** (`0x1593`, wire bytes `93 15`), not the class-local slot `2`:

```text
83                     logical chain
03                     property update item
01 65 00 00 00         target reference: lo=1, id=101
93 15                  global property ID 5523, LE
00                     tag-9 value: false
00                     end of item chain
```

As one line: `83 03 01 65 00 00 00 93 15 00 00`.

Boblox's game `walk83ItemsEx` parsed this fabricated update against the same public schema; applying it in our synthetic demo changes `Block.Anchored` from true to false. This is not proof that the number, encoding, or update direction applies to Team Create. Each additional game property update has its own `03` item; do not interpret consecutive property payloads as one update without item types.

---

## 8. Live instance and event messages

### 8.1 General chain

**[GAME]** A logical delta message can contain multiple typed items:

```text
83 [item_type][item_body] [item_type][item_body] ... [00]
```

The Boblox walker also accepts reaching the logical frame boundary without an explicit final `00` in some paths. Its runtime special-cases a message whose first item is `0x16` and sends the whole body to the batch parser; do not assume arbitrary batch items can be mixed into an otherwise ordinary chain. Many items do not have a universal length prefix; the parser must understand each body. Encountering an unknown type or unsupported value can stop the walk. Preserve the complete logical message and failure offset rather than guessing the next item boundary.

### 8.2 Types most relevant to a world model

| Game item | Observed parser shape / runtime handling | Team Create status |
|---|---|---|
| `0x02` | Spawn: instance ref, `u16` class, optional state-dependent pad byte, **two** property loops, parent ref. Loop A and B use different prefix modes. Runtime applies merged properties. | Not verified. The public game chain walker passes `has_pad=false`; its standalone spawn parser supports the other branch. Do not extrapolate the game state rule to Studio. |
| `0x03` | One property update: target ref, global `u16` property ID, value decoded using the schema-derived table and overrides. Runtime applies it. | Not verified. |
| `0x07` | Event: target ref, global `u16` event ID, then arguments described by the event schema. Runtime can dispatch events. | Not verified as the envelope for Live Scripting. |
| `0x16` | Compressed batch; see §6. | Not verified. |
| `0x23` | `u32`-shaped game clock/tick item. | Meaning and use not established for Team Create. |
| `0x01` | Boblox parses a single entity reference. Its inspected live runtime does **not** apply that item as a deletion. | Delete wire semantics/opcode are not established. |
| `0x0b`, `0x0d`, `0x25` | Additional batch/entry paths are parsed and some records applied by the game runtime. | Do not assume every creation is `0x02` or every initial object is a simple `0x16` record. |
| Pseudo-item `0x78` | Boblox labels a successfully recognized **typeless** game script-bulk payload `0x78` internally; that number is **not a wire type byte**. The runtime associates the payload with a preceding spawn. | Bytecode-related game data is not evidence of live editable Studio source. |

The public project also parses or retains some other item types; successful framing is not the same as applying their meaning. In particular, do not label `0x01` a proven Team Create delete message merely because it contains a reference.

A top-level game `0x8d` region chunk is separately recognized and its compressed payload retained; the Boblox runtime does not thereby reconstruct editable terrain. Other omissions can include external assets, shared strings, special service state and filtered properties.

### 8.3 Worked live creation: two property loops

**[DESIGN + GAME]** Here is a third fabricated object using the same particular `50 d3` game schema as §7. The public Boblox chain parser accepted this example with its **no-pad** spawn setting; this does not establish a Studio message.

```text
83                        logical chain
02                        spawn item
01 66 00 00 00            self ref: lo=1, id=102
66 01                     class 358: Part
02 01                     loop A, slot 2 Anchored: true
3b 05 43 75 62 65 32      loop A, slot 59 Name: "Cube2"
ff                        end loop A
ff                        end loop B, which is empty in this example
01 64 00 00 00            parent ref: lo=1, id=100 (Room)
00                        end chain
```

One line: `83 02 01 66 00 00 00 66 01 02 01 3b 05 43 75 62 65 32 ff ff 01 64 00 00 00 00`.

Loop A and loop B are both necessary even when one is empty. In the game reader, values in loop A use prefix mode and values in loop B use non-prefix mode. Those distinctions matter for asset/content tags, even though the simple Boolean and Name here do not exercise them. A receiver that reads only one `ff` will mistake the second `ff` for the parent reference and desynchronize.

### 8.4 Worked event: schema-selected argument

**[DESIGN + GAME]** In the same sample schema, the derived global event ID `3845` (`0x0f05`) names `Part.AttributeChanged` with one tag-2 string argument. This fabricated chain was accepted by Boblox's event parser:

```text
83                     logical chain
07                     event item
01 65 00 00 00         target ref: lo=1, id=101 (Block)
05 0f                  global event ID 3845, LE
04 54 65 73 74         tag-2 string: length 4, bytes "Test"
00                     end chain
```

One line: `83 07 01 65 00 00 00 05 0f 04 54 65 73 74 00`.

The event ID and argument tag came from this one schema. The argument is not preceded by a universal self-describing tag on the wire: the event descriptor tells the parser its type. This example only demonstrates game event parsing; it does **not** demonstrate that Live Scripting uses `0x07`, that this event is client-to-server, or that firing it changes an attribute or script source.

### 8.5 Initial completion and interleaving

**[GAME]** Boblox uses `0x84` to finalize its initial game snapshot; if a live item appears first in some game sessions, it finalizes before processing that item. That is a behavior of this receiver, not a verified Team Create completion protocol. The server may send an incomplete or filtered view even after a local marker.

**[DESIGN]** Internally distinguish `Collecting`, `Ready`, `Partial/Unsupported`, and `Disconnected`. If later changes can race with initial data, buffer and order them according to a **verified** native ordering rule or a consistent server-side revision scheme. Never save a silent mixture of old initial data and newer updates as if it were a single authoritative point in time.

---

## 9. Build an instance tree

### 9.1 Suggested internal records

**[DESIGN — illustrative Rust types, not a wire schema]**

```rust
struct SessionKey { lo: u64, id: u32 } // only if verified for the decoder in use
struct Record {
    key: SessionKey,
    class_name: String,
    properties: Vec<(String, TypedValue)>,
    parent: Option<SessionKey>,
}
// TypedValue must preserve refs, binary data, numeric widths, enums,
// CFrames, sequences, shared values, and unknown-but-length-known bytes.
```

In practice keep a session epoch with keys, a schema ID/fingerprint, and a map from the network reference to your internal ID and then to the DOM referent. A new connection may reuse a numeric key. Until proven otherwise, do not merge instances across epochs solely because their network IDs match.

### 9.2 Snapshot application algorithm

**[DESIGN]**

1. Start a staging world for the new session/revision. Decode the schema before interpreting indexed records.
2. Decode each record with exact bounds and keep the complete observed reference, class, properties and parent. Reject/flag unsupported classes or properties; count them.
3. Detect duplicate keys. Determine whether a duplicate is a retransmission, an update, or a conflict **from the verified protocol**. Until then, do not silently take the first or last one.
4. Allocate each object in an internal model, independent of parent order. In an API requiring a parent at creation, use a private staging root or resolve parents in passes; do not expose a partial tree as complete.
5. Resolve parent links; detect cycles, self-parenting and missing parents. Map known service roots only after you have evidence for how the protocol represents them. Do not automatically manufacture `Workspace` for every null parent.
6. Decode and resolve property references, shared values and asset/terrain data. Preserve unresolved keys and report them.
7. Apply known incremental items in the correct order and at the verified snapshot boundary. Ensure edits arriving during reconstruction are not lost or applied twice.
8. Validate completeness, then export a snapshot. Mark it partial if source, terrain, values, parents, hidden/default state or relevant changes are missing.

A parent can be received after its child. A reference can point to an object created later. `Name` is a property, not the object's identity. A class name by itself does not say which default service object should be reused.

### 9.3 Omitted properties and defaults

A record may not contain every property. Missing can mean default, filtered, inherited, unavailable or not yet sent; the correct interpretation depends on the actual snapshot protocol and matching reflection data. Automatically filling defaults from a different Studio build can produce a plausible but incorrect place. Track at least `known value`, `known absent/default under verified rules`, and `unknown/not received` separately when fidelity matters.

Roblox's API/class names, reflection database names, network member names, aliases, security/writeability and file serialization names need an explicit conversion layer. They are not guaranteed to match in case, type, or availability. A parsed wire value is not automatically writable into `rbx_dom_weak` with the same property spelling.

### 9.4 Ongoing lifecycle requirements

**[DESIGN]** Whatever the eventual native encodings, a collaboration model must handle:

* **Create:** allocate one object, validate class/identity and initial values, then attach it when its parent is available.
* **Set property:** validate the target and property/type, apply in the established order, resolve any referenced value and notify observers.
* **Rename/reparent:** change the property or parent relation without changing the object's stable identity; prevent parent cycles.
* **Delete:** determine the actual native semantics, including descendants and references. Internally use a tombstone/generation or equivalent to reject stale updates after deletion; do not reuse a key prematurely.
* **Reconnect:** establish whether identities persist. Otherwise start a new epoch, replace/resynchronize state, and discard stale events from the old epoch.
* **Unknown/missing:** report gaps or resynchronize; never treat unknown raw bytes as successfully applied edits.

The wire item and whether a delete cascades are **not** established for Team Create. The list above is the functionality a correct implementation needs, not a claim about native item numbers.

---

## 10. Scripts and live document operations

### 10.1 The three different things called “script data”

1. **Saved source** in a place file is text/state stored with a Script, LocalScript or ModuleScript.
2. **The open editor document** is the text currently being edited. In Live Scripting or Drafts, this may differ from a simple `Script.Source` read at a given instant.
3. **Game bytecode/bulk data** is used by a game runtime. It is not a substitute for original editable source and cannot be treated as proof that Studio sends source in that form.

The public Roblox `ScriptDocument` / `ScriptEditorService` APIs describe plugin-facing editor behavior, not the native network packet format. Roblox's Live Scripting announcement describes real-time collaboration and distinguishes it from Drafts; it also warns that direct `Script.Source` access can differ from editor-aware access. A server that only mirrors ordinary instance properties does not automatically implement shared editor buffers.

### 10.2 Pinned Studio signals: what their names do and do not prove

**[STUDIO STATIC]** The inspected pinned build contains Live Scripting signal names including:

```text
BeginEdit                BeginEditWithUri     EndEdit
SendOperation            SendAtomicOperation  SendAtomicOperationWithUri
SendAtomicUpdateResponse OpenedWithSource     FailedToOpen
KickFromEdit             BroadcastMessage
```

Selected descriptor-construction labels were observed as:

```text
BeginEdit(id, guid)
SendOperation(id, op)
SendAtomicOperation(requestId, guid, op, hash)
```

These are registration strings/argument labels in the binary. They are **not** verified packet IDs, argument wire types, sender/receiver directions, authorization rules, or a complete event sequence. The presence of `OpenedWithSource` is not proof that a particular `0x07` game event transmits source or that every client is authorized to receive it. The hash algorithm and what the hash covers are not established.

### 10.3 An observed nested operation shape

**[STUDIO STATIC + SYNTHETIC EXAMPLE]** The pinned binary's operation writer/reader supports a nested JSON structure with the fields below. This example was invented to illustrate the shape, not captured from a Studio session and not a complete network message:

```json
{
  "payload": {
    "atoms": [
      { "type": 0, "position": 5, "contents": "hé" }
    ]
  },
  "timestamps": { "local": 1, "remote": 0 }
}
```

What is known narrowly:

* The pinned reader accepts numeric atom types `0` and `1` in the inspected path.
* An atom has a numeric position and contents; the examined writer uses nonnegative 32-bit position/timestamp values.
* The operation has local and remote timestamp fields.

What is **not** known: the full operation semantics, position origin/units in the complete protocol, whether and how `contents` expresses deleted text, atom ordering rules, timestamp meaning, acknowledgements, message envelope, hash algorithm and recovery behavior. The local research JSON parser chooses strict bounds and rejects unknown fields; that is a safety policy, not a claim about native Studio validation.

### 10.4 What one isolated transform tells us

**[STUDIO STATIC / EMULATED]** For a subject atom of numeric type 0 at position `p`, an other atom at `q`, and other short-string byte length `L`, one helper from the pinned executable produced these results in 518 **synthetic** non-overflowing emulated cases:

| Other type | Result for subject position |
|---|---|
| 0 | Advance `p` by `L` when `p > q`, or when `p == q` and the helper's fourth argument is false; otherwise retain `p`. |
| 1 | Retain `p` if `p < q`; clamp to `q` for `q ≤ p < q + L`; otherwise subtract `L`. |

Example: `p=5`, `q=2`, `L=3`, other type 0 gives `8`. This is **consistent with** numeric type 0 representing an insertion and type 1 a deletion. It is not proof of the full application algorithm or native ordering. The tested model limits both strings to at most 15 bytes and rejects overflow; the fourth argument's overall meaning is unresolved. The other transform cases, splitting/merging, complete multi-atom ordering and real editor application are not established.

The public `ScriptDocument` API uses 1-based lines and 1-based UTF-8-byte columns with half-open ranges. Those are **plugin API** conventions; they do not prove that the native operation's single numeric `position` uses the same units or origin. Test non-ASCII and CRLF explicitly before adopting any conversion.

### 10.5 What a correct script service must do

**[DESIGN — internal behavior, not verified Studio messages]** Maintain a separate document per script identity, with:

```text
document ID + owning instance ID + current authoritative text
internal revision/sequence + per-client subscription and permissions
operation history or transform state + pending unacknowledged operations
persistence/checkpoint state + deletion/rename lifecycle
```

A typical internally consistent flow is:

1. An authorized collaborator opens a document; the server identifies the script, supplies a consistent initial text/revision, and subscribes the client.
2. A client edits its local buffer and sends an operation based on a known revision. Preserve its request/operation identity so retries can be deduplicated.
3. The server checks authorization, document existence, base revision, size limits and operation validity; it transforms or otherwise reconciles the edit against operations the client has not yet seen, according to a **fully specified, tested** algorithm.
4. Commit the result atomically to the authoritative text, assign an internal revision, acknowledge the origin and broadcast the correctly ordered change to other subscribers.
5. Each client reconciles the remote edit against its own pending local edits. On mismatch, lost history, invalid hash or reconnect, explicitly resynchronize rather than silently overwriting.
6. Checkpoint source and instance state consistently. Handle script deletion/rename, closed editors, access revocation, disconnects and Drafts separately as required.

These steps explain what must exist, not the actual Studio message order. A simple last-writer-wins replacement of the whole source can lose concurrent edits. Conversely, choosing some generic OT or CRDT library does not make Studio understand it: its positions, transformations, tie-breaks, operation batches, revision/ack semantics and wire envelope must match the native client.

Test at least: simultaneous inserts at the same and different positions; insert inside a concurrent deletion; overlapping deletions; repeated/empty operations; multi-atom operations; Unicode before/inside the edit; CRLF; long content; failed atomic hash; duplicate requests; stale revisions; reconnect; and script deletion while an editor is open. Convergence means every authorized participant ends with identical bytes **and** the intended changes, not merely that no parser crashed.

---

## 11. Design the collaboration server

### 11.1 Separation of modules

**[DESIGN]** A manageable Rust implementation can be organized conceptually as follows; this is documentation, not a supplied project or verified API:

```text
transport/       authenticated sessions, packet reliability, ordering, framing
native_codec/    versioned schemas and verified message/item codecs
identity/        session references ↔ internal instance IDs ↔ DOM references
world/           authoritative instances, properties, parents, references
snapshot/        consistent snapshot + ordered change stream
scripts/         authoritative text, editor sessions, operations, concurrency
storage/         journal, checkpoints, .rbxl/.rbxlx import and export
policy/          permissions, read/write filtering, validation, limits
observability/   redacted metadata, counters, diagnostics, completeness
```

Keep the codec boundary narrow. `world` and `scripts` should consume typed internal commands, not raw byte offsets. The native adapter translates wire messages only where its build-specific behavior is verified. Do not ship an invented codec as a native Studio server.

### 11.2 An internally consistent initial sync

**[DESIGN]** One practical server-side approach, independent of Roblox's unknown wire protocol:

1. Authenticate and authorize the client for this place and edit mode. Select a compatible, verified codec/build configuration.
2. Atomically obtain a consistent snapshot at internal revision `R` **and** establish a replayable change-log cursor beginning at `R + 1`. A lock or snapshot transaction can do this; the change log must retain entries until the joining client has caught up.
3. Translate the snapshot into the session's native schema, reference namespace and required ordering. Include all required shared data, service identities, assets and script/document bootstrap—not only the instances the server happens to know how to encode.
4. While sending the snapshot, collect/replay changes after `R` in order. Do not lose a mutation between snapshot selection and subscription.
5. Finish using the **verified native** completion procedure, replay queued changes exactly once as required, then switch to the live feed. If replay history was lost or the client fell behind, restart/resynchronize.

Internal revision `R` is a server design tool; there is no evidence that Studio uses this exact field or expects it on the wire. Likewise, it is unsafe to claim a `0x84` game marker, candidate trailer or fabricated `0x97` tail is the native Team Create finish sequence.

### 11.3 Processing an edit

**[DESIGN]**

```text
receive → authenticate origin → decode with verified session codec
→ check permissions and property/class/type constraints
→ deduplicate if applicable → serialize into authoritative order
→ apply one transaction and append durable journal entry
→ encode/broadcast to permitted subscribers → ack according to native rules
```

Creation, parent changes, property updates, deletion and document edits have different invariants. Validate object existence and lifecycle, limits, permitted properties, parent cycles, target references and script permissions. Do not accept a client's local DOM ID as if it were the server's network identity. Avoid feedback loops and double-application when the originating client receives an echo; whether Studio expects that echo must be learned rather than assumed.

For a property referencing another object, commit only when the reference semantics are known. A delete must not allow an older queued edit to resurrect an object accidentally. For independent clients, fan out in a consistent order; avoid holding the world lock while waiting on slow network writes. Bound per-client queues and disconnect/resync lagging clients according to an explicit policy.

### 11.4 Persistence and crash recovery

Use an internal append-only change journal or transactional store, with snapshots/checkpoints and a clear ordering relation between object state and script documents. On restart, load a known checkpoint and replay only committed changes. A save should have a revision, success/error state, and an atomic replacement strategy appropriate for the filesystem; do not truncate the only good copy before serialization succeeds.

`.rbxl` is an export/checkpoint format, not necessarily a sufficient collaboration journal: it need not contain in-flight editor operations, clients' pending acks, session IDs or the native sync state. Maintain those separately. Conversely, don't claim an exported `.rbxl` includes unsaved editor buffers unless you actually merged them at a consistent revision.

### 11.5 Generating the native snapshot is the unresolved step

A local file → DOM → proposed records pipeline is useful for experiments. It is **not** a complete native server until all of the following are known for the exact Studio build: schema generation/negotiation and tail, required initial messages, filtering, services and identity lifecycle, batch flags/trailers, all value forms, parent/reference order, actual live item/event formats, source bootstrap, authorization, and script concurrency semantics. Return an explicit unsupported error rather than sending guessed data to unmodified clients.

The earlier candidate encoder handled only selected game-schema mappings, rejects scripts and unmapped data, and on one 82-instance baseplate audit reported 565 unsupported property mappings. Passing its Boblox parser checks was useful differential testing but did not demonstrate Studio acceptance, trailer validity, or complete place fidelity.

---

## 12. Loading and saving place files

### 12.1 Local import

Rust libraries from the `rbx-dom` ecosystem provide a typed DOM and binary/XML readers/writers. For a binary local place, conceptually:

```rust
use std::{error::Error, fs::File};
fn main() -> Result<(), Box<dyn Error>> {
    let dom = rbx_binary::from_reader(File::open("MyPlace.rbxl")?)?;
    println!("Loaded {} instances", dom.descendants().count());
    Ok(())
}
```

This reads **your local file**. It does not download a server's place or establish network IDs. `.rbxlx` requires the XML reader/options; be explicit about its unknown-property handling. File readers and reflection databases have version-dependent support, and some unknown on-disk types may be skipped. A file that loads and reserializes is not necessarily lossless.

The earlier research package also includes `inspect-place`:

```sh
cargo run --locked --features place -- inspect-place "path/to/MyPlace.rbxl" --tree
```

That command inspects a local file; it does not send it or sync with Studio. Its local DOM references are not established Team Create references.

### 12.2 Export reconstructed data

After resolving the records and document text that you actually have:

```rust
rbx_binary::to_writer(output_file, &dom, dom.root().children())?;
```

Use the exact library version/API you install. Supply correct top-level instances and typed property variants, resolve DOM references, and check the writer's error. For scripts, put verified editable text into the appropriate supported representation; do not reinterpret an opaque 16-byte network value as source. Save a completeness report alongside the export when you know some data is missing.

Reopen the result with an independent reader or a second process and verify classes, names, parent links, property types/values, references and source. Opening a file successfully is only a syntax/compatibility check; compare against an authorized source of truth to establish fidelity. Preserve original file backups.

### 12.3 When a complete original file is impossible

An `.rbxl` reconstructed from a client-visible stream may differ from the original even when parsing is perfect: server-side filtering, security, omitted defaults, non-replicated metadata, source restrictions, unsent terrain/assets or unavailable history can make the original unrecoverable from that stream alone. Do not promise a byte-identical or complete original without evidence.

---

## 13. Runnable Rust example: records to `.rbxl`

This is a **small synthetic integration example**, not a network client/server. It takes the two **uncompressed** records and the one fabricated game-style update from §7, decodes only their known class/member bindings, applies the update, resolves a child that may be listed before its parent, writes `decoded-example.rbxl`, and reopens it to check `Room → Block`, `Anchored = false`.

It deliberately does **not** implement `0x97` schema parsing, Zstandard, transport, the `0x16` batch wrapper/trailer, shared names, other properties, script editing, real service roots or Team Create. Numeric IDs are hardcoded only to make this one example understandable; a real decoder must use the current verified schema and contexts.

Create a small Cargo project and add:

```toml
[dependencies]
rbx_dom_weak = "4.2.0"
rbx_binary = "3.0.1"
```

Put this in `src/main.rs` and run `cargo run`. It creates or overwrites `decoded-example.rbxl` in the current directory.

```rust
use std::{collections::HashMap, error::Error, fs::File};
use rbx_dom_weak::{types::Variant, ustr, InstanceBuilder, WeakDom};

type Key = (u64, u32); // Test key only; native Team Create identity is unknown.
type R<T> = Result<T, String>;

#[derive(Debug)]
struct Node {
    id: Key,
    parent: Option<Key>,
    class: &'static str,
    name: String,
    archivable: Option<bool>,
    anchored: Option<bool>,
}
struct Cursor<'a> { data: &'a [u8], pos: usize }
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or("offset overflow")?;
        let bytes = self.data.get(self.pos..end).ok_or("truncated input")?;
        self.pos = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> R<u8> { Ok(self.take(1)?[0]) }
    fn u16_le(&mut self) -> R<u16> {
        let b = self.take(2)?; Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32_le(&mut self) -> R<u32> {
        let b = self.take(4)?; Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn varint(&mut self) -> R<u64> {
        let mut value = 0u64;
        for i in 0..10 {
            let b = self.byte()?;
            if i == 9 && b > 1 { return Err("varint overflow".into()); }
            value |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 { return Ok(value); }
        }
        Err("unterminated varint".into())
    }
    fn key(&mut self) -> R<Option<Key>> {
        let ns = self.varint()?;
        Ok(if ns == 0 { None } else { Some((ns, self.u32_le()?)) })
    }
    fn boolean(&mut self) -> R<bool> {
        match self.byte()? { 0 => Ok(false), 1 => Ok(true), _ => Err("invalid bool".into()) }
    }
    fn short_name(&mut self) -> R<String> {
        let n = self.byte()?;
        if n >= 127 { return Err("extended/shared Name not implemented".into()); }
        String::from_utf8(self.take(usize::from(n))?.to_vec()).map_err(|_| "invalid UTF-8".into())
    }
    fn done(&self) -> bool { self.pos == self.data.len() }
}

// These bindings describe ONLY the two fabricated records below. In a real
// decoder derive class/slot/type names from the CURRENT schema, not constants.
fn snapshot(data: &[u8]) -> R<Vec<Node>> {
    let mut c = Cursor { data, pos: 0 };
    let mut nodes = Vec::new();
    while !c.done() {
        let id = c.key()?.ok_or("null instance reference")?;
        let cls = c.u16_le()?;
        let class = match cls { 224 => "Folder", 358 => "Part", _ => return Err("unknown class".into()) };
        let (mut name, mut archivable, mut anchored) = (None, None, None);
        loop {
            match c.byte()? {
                0xff => break,
                2 if cls == 224 => archivable = Some(c.boolean()?),
                6 if cls == 224 => name = Some(c.short_name()?),
                2 if cls == 358 => anchored = Some(c.boolean()?),
                59 if cls == 358 => name = Some(c.short_name()?),
                _ => return Err("unknown member: cannot safely skip its value".into()),
            }
        }
        let parent = c.key()?;
        nodes.push(Node { id, parent, class, name: name.ok_or("missing Name")?, archivable, anchored });
    }
    Ok(nodes)
}

// ONLY one Boblox-style 0x03 property update; not a Team Create decoder.
fn apply_update(data: &[u8], nodes: &mut [Node]) -> R<()> {
    let mut c = Cursor { data, pos: 0 };
    if c.byte()? != 0x83 || c.byte()? != 0x03 { return Err("expected 83 03".into()); }
    let id = c.key()?.ok_or("null update reference")?;
    if c.u16_le()? != 5523 { return Err("unknown global property ID".into()); }
    let anchored = c.boolean()?;
    if c.byte()? != 0 || !c.done() { return Err("bad item terminator/trailing data".into()); }
    let node = nodes.iter_mut().find(|n| n.id == id).ok_or("unknown update target")?;
    if node.class != "Part" { return Err("property/class mismatch".into()); }
    node.anchored = Some(anchored);
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Concatenated UNCOMPRESSED records; not transport or a full 0x16 batch.
    let records: &[u8] = &[
        0x01,0x64,0,0,0, 0xe0,0, 0x02,1, 0x06,4,b'R',b'o',b'o',b'm', 0xff,0,
        0x01,0x65,0,0,0, 0x66,1, 0x02,1, 0x3b,5,b'B',b'l',b'o',b'c',b'k', 0xff,1,0x64,0,0,0,
    ];
    let update: &[u8] = &[0x83,0x03,1,0x65,0,0,0,0x93,0x15,0,0];
    let mut nodes = snapshot(records)?;
    apply_update(update, &mut nodes)?; // Block.Anchored: true -> false

    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let mut refs = HashMap::new();
    let mut pending: Vec<&Node> = nodes.iter().collect();
    while !pending.is_empty() {
        let before = pending.len();
        let mut later = Vec::new();
        for n in pending {
            if refs.contains_key(&n.id) { return Err("duplicate instance ID".into()); }
            let parent = match n.parent {
                None => dom.root_ref(), // This demo grafts the null sentinel to its local root.
                Some(p) => match refs.get(&p) { Some(r) => *r, None => { later.push(n); continue; } },
            };
            let mut b = InstanceBuilder::new(n.class).with_name(&n.name);
            if let Some(v) = n.archivable { b = b.with_property("Archivable", v); }
            if let Some(v) = n.anchored { b = b.with_property("Anchored", v); }
            refs.insert(n.id, dom.insert(parent, b));
        }
        if later.len() == before { return Err("missing parent or parent cycle".into()); }
        pending = later;
    }
    let path = "decoded-example.rbxl";
    rbx_binary::to_writer(File::create(path)?, &dom, dom.root().children())?;
    let check = rbx_binary::from_reader(File::open(path)?)?;
    let folder = check.descendants().find(|n| n.class.as_str() == "Folder").ok_or("missing Folder")?;
    let part = check.descendants().find(|n| n.class.as_str() == "Part").ok_or("missing Part")?;
    assert_eq!(folder.name, "Room");
    assert_eq!(part.name, "Block");
    assert_eq!(part.parent(), folder.referent());
    assert_eq!(part.properties.get(&ustr("Anchored")), Some(&Variant::Bool(false)));
    println!("Roundtrip OK: Room > Block; Anchored=false");
    Ok(())
}
```

**Validation actually performed:** this exact snippet compiled with Rust 1.99.1 and the versions above; it wrote and reopened the file and printed `Roundtrip OK: Room > Block; Anchored=false`. Independently, the public Boblox parser accepted the synthetic records and `0x03` update against the referenced public game schema. Neither test ran Studio or authenticated to Team Create.

---

## 14. A practical integration plan for your port

### Phase A — prove where your data is

1. Identify which transport path your Rust port uses, and where complete logical messages are delivered. Instrument message ID/size/order without credentials.
2. Confirm the connection is in collaborative **edit** mode rather than a game/playtest path. Your claim that join works is a useful starting point; it does not tell us what the replication receiver does next.
3. Compare message IDs, ordering and sizes to §4–§8 **as hypotheses**. If no `0x97` appears, the schema may use another channel, shape, build, phase or already-negotiated state; do not force a game parser onto the bytes.
4. Preserve session-local schema and messages securely where you are authorized. Fingerprint them, track direction, and never share tickets or keys. If you cannot obtain plaintext logical messages, an encrypted datagram alone will not let this parser work.

### Phase B — reconstruct a limited initial tree

1. Port or independently implement the exact **matching** schema reader and value readers. Compare schema counts, member names, record counts and EOF offsets, not just whether parsing returns success.
2. Parse known instance batches; distinguish flags 1/5 from record flags only if the session evidence actually supports the same layout.
3. Save decoded records as typed internal data. Use a map and deferred references, resolve parent graph, surface unknowns and compare against an authorized known test place.
4. Export an `.rbxl` only with a completeness report. Start with a deliberately small place; expand one class/property/type at a time.

### Phase C — synchronize instance edits

Using your own authorized two-client test place, compare a baseline with one controlled change at a time: create a Folder, create a Part, change a scalar, rename, reparent, set an object reference, delete, then reconnect. Record the **logical** messages and resulting authoritative state; test both directions and ordering. Determine the actual native item/event types and per-session identity behavior before implementing them. The list is a future verification plan; no live Studio access was used to produce this guide.

### Phase D — add script documents

1. Establish how a script is identified and opened, how initial source is delivered, and which event/transport carries the nested operation.
2. Collect isolated authorized edits: ASCII insert/delete, multi-byte text, CRLF, several atoms, simultaneous edits and reconnect. Compare directions, positions, timestamps, hashes, acknowledgements and actual final buffers.
3. Verify type meanings and full transform cases, including overlapping deletes and tie-breaks. Differentially test against the exact target Studio build, not only the isolated helper.
4. Implement document state/revision, pending operations, recovery and persistence, then confirm two unmodified clients converge without losing edits.

### Phase E — make a server

Only once the appropriate native session, schema, bootstrap, snapshot and live-edit encoders are known, connect the internal world/script modules to the verified codec and test a fresh Studio session end to end. The previously supplied candidate encoder can help explore **game-style** mapping; it cannot fill the unknown native pieces automatically.

---

## 15. Verification and test matrix

### 15.1 Parser tests

* Golden schema fixtures from the exact target build/session: verify every table count and selected member/event lookup. Retain exact schema bytes and expected tail length.
* Unit tests for truncated inputs at every byte boundary, varint overflow, bad class/member/event indices, invalid lengths, decompression errors, unsupported tags and name-table bounds.
* Differentially compare logical message parsing, consumed byte counts and typed output with a known receiver **for the same protocol/build**. A second parser using the same incorrect assumption is not independent proof.
* For batches, check compressed/decompressed lengths and all understood trailer fields. `parseDeltaBatch` accepting a compressed payload is not trailer validation.
* Fuzz bounded cursor, value, schema and item readers. Verify errors cannot cause uncontrolled memory growth or panic.

### 15.2 World/state tests

* Multiple batches; child before parent; reference before target; duplicate IDs; duplicate names; cycles; missing parent; stale update after delete; ID reuse and reconnect.
* Every supported property value: exact bytes, semantic value, DOM type, file roundtrip, and boundary cases. Include binary strings, empty values, shared names/strings, assets, CFrames, enums and references.
* Snapshot/live overlap: an edit made during snapshot must appear exactly once in the final world, not be dropped or applied twice.
* A completeness manifest: record count, unsupported properties/classes, missing parents/refs/assets/terrain/scripts, source status, schema fingerprint and session/revision boundary.

### 15.3 Script/concurrency tests

For any chosen internal algorithm, test single edits, Unicode units, atomic batches, concurrent same-position insertions, overlap, stale base revisions, replay, duplicates, lost acknowledgements, reconnect and convergence. Then test the **actual native codec and behavior** separately against unmodified Studio. Passing an internal OT test does not verify Studio protocol compatibility.

### 15.4 End-to-end acceptance criteria

A claim of a verified Team Create implementation should require at least:

1. An unmodified target Studio build authenticates and joins the Rust server in edit mode.
2. A known `.rbxl` with representative instances, properties, references, services, scripts and terrain opens with no unexplained errors or omissions.
3. Two clients create, edit, reparent and delete objects bidirectionally and converge; reconnect recovers correct state.
4. Two clients edit the same script concurrently, see identical intended text, handle conflicts/recovery, and persist/reopen it.
5. A saved/reopened place matches the authoritative state at the declared revision, with unsupported items explicitly reported.

None of these end-to-end Studio criteria has been tested here.

---

## 16. Troubleshooting

| Symptom | Likely layer to inspect | Do not assume |
|---|---|---|
| Join succeeds, but no objects | Correct receive callback, edit mode, stream/channel, framing, whether a replication consumer is installed | That authentication automatically builds the place. |
| First byte is not `97`/`83` | Are you looking at a whole logical message or at transport, ciphertext, framing or a different build/session? | That the game IDs must appear in Team Create. |
| `0x97` inflate error | Header alignment, version, input extent, compression, expected size, memory limits | That a random `97` byte marks a schema. |
| Classes look wrong after parsing | Exact schema/build, class index endianness, mismatch between schema and batch | That a class index is stable across builds. |
| First few values parse, then offsets drift | Tag decoder, class-local slot vs global ID, prefix mode, special overrides, unsupported value | That the next byte is a safe new record boundary. |
| Names missing or garbage | Name tag 49 literal/shared forms and correct name table; binary vs UTF-8 | That flag-1 shared strings and schema shared names are the same. |
| Tree has wrong roots | Parent sentinel policy, service identities, missing parents, cycles, filtering | That null parent always means Workspace. |
| Updates target wrong object | Preserve full identity and session epoch; inspect actual ID lifecycle | That a local DOM referent equals a network ID. |
| Property updates fail or desynchronize | `0x03` is one game item per update; global ID/table/override and event order | That snapshot slot equals live global ID. |
| Files open but scripts are empty/old | Whether editor source was ever received, source vs bytecode vs Drafts, save revision | That opaque tag-47 bytes are source text. |
| Objects change but two clients diverge | Snapshot/live race, message order, dropped unsupported items, echo/ack rules, script concurrency | That one successful parse or single-client test proves synchronization. |
| Batch parses but Studio refuses it | Schema tail, flags, trailer, handshake, order, identity, filtering, version and native mode | That Boblox's parser validated the trailer or Studio wire. |

---

## 17. Security and resource limits

* Only inspect sessions and place/source data you are authorized to access. Do not expose or request session credentials or cryptographic keys in logs or reports.
* Limit raw frame size, decompressed size and Zstandard window, schema entries/strings, nesting depth, per-record and per-session memory, queued changes, operation atom count and source length. Use checked arithmetic. A claimed uncompressed size is untrusted input.
* Preserve unknown data only within bounded local storage and with appropriate access controls. If an unknown value's extent is unknown, retain the containing message rather than pretending to skip that value.
* Version and authenticate sessions; validate permissions for each edit and script document. A client-supplied target ID is not proof of authority.
* The place-file readers used here are not a security sandbox for hostile files; decompression and unsupported formats can consume resources. Process untrusted files in an isolated worker with operating-system limits.
* Back up files and use atomic save/checkpoint practices. Protect script source and creator data at rest and in diagnostic captures.

---

## 18. What remains unknown for native Team Create

The following gaps are **blocking facts**, not implementation details that can safely be guessed from the game receiver:

| Area | Known evidence | Still needed for unmodified Studio |
|---|---|---|
| Session/bootstrap | Your port is reported to join; pinned Studio contains Team Create-related mode/join strings. | Exact target build, accepted authentication/authorization, transport and handshake, channels, flags, message ordering and reconnect semantics. Your code/capture has not been reviewed. |
| Schema | Public game `0x97` prefix and ordered tables; a substantial tail is opaque. | Actual Team Create schema delivery, tail, build compatibility, negotiation/filtering and whether the game layout applies at all. |
| Initial world | Game `0x83/0x16` record batches; pinned Studio has kind-1/5 builders. | Native instance builders/flags and full trailers; required root/service mapping, shared data, assets, terrain, filtering and completion procedure. |
| Identity/lifecycle | A game reference is `lo` plus optional u32; some game live items are parsed. | Native namespaces, identity persistence, create/update/reparent/delete messages and semantics, duplicate handling, reconnect. |
| Properties/events | Game tables and several value readers; `0x03` and `0x07` have observed game shapes. | Team Create property/event tables, all value forms and overrides, directions, permissions, ordering and the live-script envelope. |
| Script editing | Studio names/labels, a nested JSON shape, and a narrow emulated type-0 transform. | Source delivery, exact event framing/types, atom semantics/units, timestamp/hash/ack meanings, complete multi-atom transforms and convergence behavior. |
| Persistence | Local file readers/writers and a tested synthetic export. | Proof all necessary state is present and saved at a consistent collaborative revision; behavior for omitted/unavailable data. |

An authorized, minimal two-client trace with a precisely recorded action sequence and matching Studio build would be the most direct way to resolve many of these. Static analysis of additional pinned-build code can narrow hypotheses, but function names or isolated emulation are not a substitute for an end-to-end result. Keep the native adapter disabled/fail-closed when these facts are absent.

---

## 19. Sources and provenance

* Public Boblox game receiver, pinned revision: [repository](https://gitlab.com/kingdudely/Boblox/-/tree/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287), [schema and snapshot reader](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/solve/repl_snapshot.cpp), [value reader](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/solve/repl_core.cpp), [live item reader](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/solve/repl_stream.cpp), [runtime application](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/engine/live_runtime.cpp), [project findings](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/FINDINGS.md). Some older header/findings comments are superseded by later code/findings; this guide follows the pinned implementations where noted. No license was found in the inspected checkout; check its terms before copying source or captures.
* Pinned Studio static evidence in the previously supplied local research project: `roblox-team-create-rust/docs/FINDINGS.md`, `docs/CANDIDATE_SNAPSHOT.md`, `docs/VALIDATION.md`, `src/script_operation.rs`, `src/operation_transform.rs` and `evidence/`. The studied release was Windows Studio `0.745.0.7451137`, deployment `version-7062aec9dbe74b14`, EXE SHA-256 `781b36ed6fb62e61292b9e6a64327280c7da12b00f3ea8b2c91a3f4834f860ec`. Static code addresses are specific to that pinned image; no Studio session was executed.
* [Official Studio archive for the pinned deployment](https://setup.rbxcdn.com/version-7062aec9dbe74b14-RobloxStudio.zip) and [matching API dump](https://setup.rbxcdn.com/version-7062aec9dbe74b14-API-Dump.json). The archive and executable were downloaded from the CDN and hash-checked for the earlier static research.
* [Roblox Live Scripting announcement](https://devforum.roblox.com/t/live-scripting-beta/2640607), [collaboration documentation](https://github.com/Roblox/creator-docs/blob/main/content/en-us/projects/collaboration.md), [ScriptDocument API](https://create.roblox.com/docs/reference/engine/classes/ScriptDocument), [ScriptEditorService API](https://create.roblox.com/docs/reference/engine/classes/ScriptEditorService), and [DraftsService API](https://create.roblox.com/docs/reference/engine/classes/DraftsService). These are behavior/API references, not native packet specifications.
* [`rbx_binary` 3.0.1 reader](https://docs.rs/rbx_binary/3.0.1/rbx_binary/fn.from_reader.html), [`to_writer`](https://docs.rs/rbx_binary/3.0.1/rbx_binary/fn.to_writer.html), and [`rbx-dom` repository](https://github.com/rojo-rbx/rbx-dom) for local file and DOM operations.
