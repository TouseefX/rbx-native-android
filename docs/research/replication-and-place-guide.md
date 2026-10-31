# Team Create replication, place reconstruction, and live editing

**A research and implementation guide for the receiving client and a prospective Rust server**  
**Evidence reviewed:** 31 October 2026  
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
| **[STUDIO EMULATED]** | Selected machine-code functions executed on synthetic inputs under Unicorn, with specified helpers mocked. This is not a Studio session or a network capture. |
| **[DESIGN]** | An implementation recommendation or an invented example. It is not a native Roblox packet. |
| **[UNKNOWN]** | Required for the real target but not established by available evidence. |

The game receiver, the particular Studio executable, and your Rust port are **not known to be the same build or protocol**. You said your port can join Team Create; its source and a Team Create capture were not supplied. Therefore there is no verified native-Team-Create decoder or server implementation in this document. Where the native details are unknown, the guide says what evidence is missing rather than inventing bytes.

### Quick answers

1. **Where is the place?** It may be reconstructed from initial replicated state, but the exact Studio bootstrap is not established and need not be a downloadable original `.rbxl`. If records are sent, gather them, resolve parents and references, process relevant changes and script source, then write a new file.
2. **Where do I hook my Rust Boblox port?** At the callback that receives a **complete, decrypted, already transport-reassembled logical replication message**, before a game runtime discards or simplifies it. Which callback that is depends on your port.
3. **How do I know the IDs and types?** In Boblox's observed game format, the session schema supplies ordered class/member/event tables. Snapshot property slots are class-local; live property IDs are global and schema-specific.
4. **How do I synchronize later edits?** Apply creation, property changes, parenting, deletion and editor operations in protocol order, with a consistent initial snapshot and stable object identities. The corresponding **Team Create wire formats are not all known**.
5. **How do I edit scripts?** Treat editor documents and their concurrent operations as a separate subsystem; `Script.Source`, compiled game bytecode and an editor buffer are not interchangeable. Studio code directly serializes an operation and dispatches it through a `SendOperation` descriptor, but its network envelope and complete semantics remain unknown (§11.7.3).
6. **Can this already be made to work with unmodified Studio?** Not reliably from the available evidence alone. The missing native bootstrap, schema/tail, instance lifecycle and script event/operation details are listed in §18.
7. **What can I verify about `Collaborator`?** The pinned build exposes nine class-specific fields and local service/creation/update paths; in specific creation and status paths, the service signal uses the same ID as `UserId`. This does not establish their Team Create wire encoding or delivery (§11.8).

### Contents

1. [Whole-system map](#1-whole-system-map)
2. [Start in your existing client](#2-start-in-your-existing-client)
    * [First join, player identity, and initial updates](#24-first-join-player-identity-and-initial-updates)
    * [Game-schema player and collaborator fields](#25-player-and-collaborator-descriptors-in-a-public-schema)
    * [Treat the initial replication as a baseline](#26-using-the-first-replication-as-your-baseline)
3. [Binary primitives and framing](#3-binary-primitives-and-framing)
4. [Schema message and tables](#4-schema-message-and-tables)
5. [Value formats](#5-value-formats)
6. [Initial instance batches](#6-initial-instance-batches)
7. [Record-by-record worked example](#7-record-by-record-worked-example)
8. [Live instance and event messages](#8-live-instance-and-event-messages)
    * [Synthetic player creation and update](#86-synthetic-player-creation-and-update)
9. [Build an instance tree](#9-build-an-instance-tree)
10. [Scripts and live document operations](#10-scripts-and-live-document-operations)
    * [Text cursors, selections, and viewport presence](#106-text-cursors-collaborator-selections-and-3d-viewport-presence)
    * [Decode an initial Collaborator and later changes](#107-worked-collaborator-baseline-and-updates)
11. [Design the collaboration server](#11-design-the-collaboration-server)
    * [How my edits reach the server and other collaborators](#116-how-my-edits-reach-the-server-and-other-collaborators)
    * [Actual Studio outgoing paths: properties, scripts, cursors and presence](#117-decompilation-how-studio-constructs-outgoing-changes)
    * [Verified Collaborator fields, identity and lifecycle](#118-verifying-collaborator-fields-identity-and-lifecycle)
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
| Collaborator presence | Studio documents colored cursors, selections and collaborator locations; the pinned API dump exposes relevant hidden descriptors. Their native message layouts are not established. |
| Join and authenticate an unmodified Studio client against a new Rust server | Not implemented or tested here. Your own claimed join path was not inspected. |
| Complete Team Create snapshot, live edits, script source and two-way editing | Not established or tested. |

### 1.5 Does Studio download an `.rbxl` and then update it?

The essential conceptual process is **initial state → ongoing changes**, but the exact native bootstrap has not been captured here. Do not assume the client always downloads a complete `.rbxl` first:

```text
[DESIGN — possible server architecture, not a verified Roblox sequence]

existing server state or .rbxl → server's in-memory place
          → client gets enough initial state to build its view
          → client receives and sends live edits
          → server periodically persists a consistent state
```

A server can load its own `.rbxl` as the starting state without sending that file verbatim to clients. A client can instead reconstruct the initial world from snapshot records, and can fetch assets or other data separately. Caching, file downloads or other bootstrap paths might also be involved; the evidence in this guide does not establish every step of a real Studio join. In particular, Boblox's game snapshot is not proof of Team Create's exact wire format. The reported Rust client does not automatically need to open a local `.rbxl` just to receive replication; it needs whatever initial synchronization that particular server actually uses.

If you already possess a local place and want to use it as a baseline, the proposed server can load it. It still has to establish a consistent revision and receive/apply everything changed since that baseline. Two clients starting from different or stale files without such a synchronization point can diverge or duplicate objects. For actual native Studio, whether that baseline is accepted and how it is reconciled remain unknown.

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

### 2.4 First join, player identity, and initial updates

A newly joined participant needs more than place objects. The connection also needs to know **which participant this is**, what other participants are present, the current state of the place and editor, and how to proceed from initial state to ongoing changes. My earlier high-level description did not make that distinction clear enough.

**[GAME — public Boblox reference, not a Studio Team Create sequence]** At the *logical application-message* boundary, Boblox distinguishes these things. This table is a map of roles, **not a mandatory sequence**; actual messages can be interleaved and differ between transports and builds.

| Message/item | Direction in the cited game path | What it contributes / limitation |
|---|---|---|
| `0x90` | Client → server | Client-info/join payload. The game builder merges join information including `UserId` into a template; the payload can contain sensitive session data. It is not an initial world snapshot, and a client-supplied name or ID is not independently authenticated merely because it appears here. |
| `0x93` | Server → client | In the later Boblox finding, this sample is a list of server flag name/value pairs—not a player list or snapshot. |
| `0x81` | Server → client | Application-level peer initialization: seven prefix bytes, a LEB128 peer ID, then a tail Boblox keeps opaque. It is **not** itself the account `UserId` or a whole Player record. |
| `0x97` | Server → client | Join schema and tables in the inspected game formats. It describes possible classes/properties/events; by itself its declarations are not proof that any particular Player or Collaborator object has been sent. |
| `0x83/0x16` | Server → client | Compressed batches; in observed kinds 2/3/4 these include initial instance records. Some kinds hold other data. Their presence, values, and ordering depend on the session. |
| `0x83/0x02`, `0x83/0x03`, `0x83/0x07` | Server → client in the inspected receiver | Generic instance spawn, property update, and schema-selected event, respectively. A Player *can* be a spawned instance; `0x02` is not exclusively a player record. Some state can arrive after the initial batches. |
| `0x84` and `0x04` | Server → client marker; client → server response in Boblox's archived RbxTransport game path | Boblox uses `0x84` to finish snapshot staging and replies with a framed logical `[04][marker:u32 LE]`, echoing the marker from `[84][marker:u32 LE]`. Its research reports the response was tolerated but did not by itself solve the missing-character problem. This is distinct from a transport-level RakNet datagram ACK. Do not assume Studio uses the same marker or response. |

The numeric bytes `0x81` and `0x83` also occur at **lower RakNet layers** with other meanings. An encrypted UDP datagram starting with one of those values is not necessarily the application message shown above. Reassemble and decrypt first, and log direction, layer and logical message boundaries.

A practical Boblox-port pitfall: in the archived `quic/session.cpp`, `peer_assigned` becomes true on post-answer channel traffic without itself decoding a `0x81` peer-init message. `LiveRuntime::onPeer` separately calls `parsePeerInit`. Therefore a generic “peer=yes” status or a successful transport join is not by itself proof that a peer-init record, a player, or a complete world was received.

Another pitfall is the older `parsePlayerRecs` helper. It scans particular `0x83/0x02` payloads for an avatar URL, an entity pattern and a name; its test covers a particular game sample. The newer `parseSpawnItem` is schema-driven and handles generic classes. **Do not implement that URL-scanning heuristic as a universal Player packet format**, and do not infer that it describes Studio collaborator presence.

#### Identities that must not be conflated

| Identity | Purpose in an implementation | Not automatically equal to |
|---|---|---|
| Authenticated account/user ID | Identifies a person according to your session authorization. Bind it to the connection only after validation. | Peer ID, instance reference, or a display name. |
| Peer/session ID | Identifies a network peer for a particular protocol/session, if that protocol uses one. | The user's stable account ID. |
| Network instance reference | Points to an object in a protocol/session identity namespace. In the game parser it has a `lo` component and an optional `u32`; the full namespace semantics are unresolved. | User ID or `.rbxl` referent. |
| Document ID / local DOM referent | Identifies a script document or in-memory/file object in its own namespace. | A peer, Player, or collaborator ID without an explicit mapping. |

Boblox's game runtime tries to look up `Players.LocalPlayer` using a separately supplied authenticated user ID, but its current implementation **falls back to the first Player child when no match is found**. That fallback can point at the wrong participant; do not silently use it as a reliable identity rule in your Rust port. The game runtime synthesizes `PlayerAdded`/`CharacterAdded` from its applied tree. Those local callbacks are not proof that an equivalent wire event must be sent on Studio join; double-firing an inferred event and a received event could duplicate notifications.

#### What your Rust server needs conceptually

**[DESIGN — recommended internal sequence, not verified Studio packets]**

1. Validate the incoming session and bind it to an authorized user and permissions. Allocate a session epoch; do not trust an arbitrary claimed username/user ID as proof of authorization.
2. Atomically capture a current place/document revision **R** and subscribe this session to changes after R, or use another design that cannot lose changes between snapshot creation and subscription. Preserve any required service and instance references.
3. Initialize the joining participant with the protocol-required identity, current place state, relevant open-document state, and the presence of authorized existing participants. Notify existing participants of the new participant as appropriate. Whether Studio represents these with Player/Collaborator objects, events, or another channel is **unknown**.
4. Queue changes produced while the initial state is being sent; replay changes after R in authoritative order without duplicates, resolving references as needed. Treat presence/camera/caret updates as transient and separately authorized even if the native implementation multiplexes them with other messages.
5. Move the client to an internal synchronized state only once the server's actual bootstrap and any required native acknowledgement conditions are satisfied. On disconnect/reconnect, remove or replace the old presence epoch and avoid retaining stale cursors or identities.

Do not persist the session's users, local peer IDs, or transient presence as ordinary place content merely because you persist the `.rbxl`. Do not claim the above ordering or message IDs are native Team Create facts without target-build evidence.

### 2.5 Player and collaborator descriptors in a public schema

**[GAME — independently parsed for this guide]** Boblox's checked-in `run/join97_new_frame.bin` is a 54,105-byte `0x97` game-schema message with magic `50 d3`, SHA-256 `2177e3975df2ad63b9ce01e61fa54d468436a55bcf732d52eef828738f63602d`. Using the pinned `parseJoinOuter` and `parseJoinSchema`, its table contains these **zero-based** class indices:

| Descriptor in this game schema | `50 d3` class index | Selected relevant members or events |
|---|---:|---|
| `Collaborator` | 141 | `CFrame`, `CollaboratorColor3`, `CurDocGUID`, `CurScriptLineNumber`, `IsIdle`, `Status`, `UserId`, `Username` |
| `CollaboratorsService` | 142 | `CollaboratorIdleUpdate`, `CollaboratorInstanceCreatedSignal`, `CollaboratorInstanceDestroyedSignal`, status signals |
| `LiveScriptingService` | 303 | `BeginEdit`, `OpenedWithSource`, `SendOperation`, atomic-operation descriptors |
| `Player` | 380 | `UserId`, `Name`, `DisplayName`, plus CloudEdit-related descriptors |
| `Players` | 386 | `PlayerAdded`, `CloudEditApplyEditsMessage`, `TeamCreateServerMessage` descriptors |

The **schema declarations are useful leads, not captured Team Create actions**. They do not prove those classes are instantiated, a particular event fires, or Studio accepts the same IDs. For context, Boblox reports one captured game snapshot with **no Player or Humanoid** even though those class descriptors existed. A schema describes vocabulary, not a list of currently connected people.

There is stronger evidence for players in a *different public game message*: I parsed Boblox's `tests/vectors/repl/msg02.bin` (1,306 bytes; SHA-256 `7f2b674bc857a03f8d26ce2783c9c0283794026e62ee98344dc927e4c93e9cec`) against the `a3 cf` schema in `run/repl_native97.bin`. The pinned schema-driven chain parser consumed all 1,306 bytes and reported **33 items: 32 generic spawns and one `RemoteEvent.OnClientEvent` event**. Exactly one spawn had class `Player`, with properties including `DisplayName`, `UserId`, `Name`, `CharacterAppearance`, and `User`. The same chain also contains other classes, including input bindings. This is an observed **game** message, not evidence that it was the first replication message or that native Team Create uses the same contents. It does show why treating the entire `0x83` message or every `0x02` item as a dedicated player-info packet is wrong.

For the `50 d3` sample, these member indices/tags are concrete. A snapshot/spawn uses the class-local slot; the game receiver derives a global property ID for a `0x03` update from the full ordered schema (§4.4). The listed tag is the **schema tag**; consult the live-update override table as well.

| Class.member | Local slot | Schema tag | Derived global property ID |
|---|---:|---:|---:|
| `Player.DisplayName` | 24 | 2 | 6022 |
| `Player.UserId` | 49 | 44 | 6047 |
| `Player.Name` | 57 | 49 | 6055 |
| `Collaborator.CFrame` | 0 | 27 | 2099 |
| `Collaborator.CollaboratorColor3` | 2 | 19 | 2101 |
| `Collaborator.CurDocGUID` | 3 | 2 | 2102 |
| `Collaborator.CurScriptLineNumber` | 4 | 10 | 2103 |
| `Collaborator.IsIdle` | 5 | 9 | 2104 |
| `Collaborator.Status` | 6 | 7 | 2105 |
| `Collaborator.UserId` | 7 | 44 | 2106 |
| `Collaborator.Username` | 8 | 2 | 2107 |

The `Status` schema member's enum index is 132, which resolves to the name `CollaboratorStatus` **in this schema**. The actual role, source, delivery and interpretation of those fields in Studio remain unknown.

As a concrete version-skew check, another public Boblox fixture, `run/repl_native97.bin` (magic `a3 cf`, SHA-256 `3bba2d30802e8f5fe3bd5ddc192872cefcfa1cb21353c05f10e89fa88ffc35fe`), gives class indices **140** for `Collaborator` and **375** for `Player`, and global IDs **2045** for `Collaborator.UserId` and **5907** for `Player.UserId`. The names are familiar but the IDs differ. Parse the schema for each compatible session; never hardcode these sample numbers for Studio.

A further build distinction: `Player.CloudEditSelectionChanged` appears in both the Boblox game schema and the pinned Studio API dump. The game schema also contains `Player.CloudEditCameraCoordinateFrame`, `Player.CloudEditPlayerActive`, `Players.CloudEditApplyEditsMessage`, and `Players.TeamCreateServerMessage`; those four names are **absent from the pinned Studio 0.745.0.7451137 API dump** I checked. An API-dump omission does not prove the implementation or wire cannot use a name, but it is a warning against copying game descriptors or IDs into a Studio server as if they were verified.


### 2.6 Using the first replication as your baseline

The first replication phase is important: it is where a receiver can build a **current view of the session**, rather than just waiting for a downloadable `.rbxl`. In the kind of workflow described by the user, the receiver needs to account for all of these together:

```text
initial synchronization
  ├─ session/peer identity and current participants
  ├─ world: services, instances, properties, references, parents, assets …
  ├─ current collaborator/presence information, where actually supplied
  ├─ script source/document state, if and where available
  └─ any required synchronization markers or other control messages
       ↓
ordered updates during and after the baseline
  ├─ instance create/change/reparent/delete
  ├─ participant joins/leaves and presence changes
  └─ live script/document edits
```

**[DESIGN]** These are logical categories, not proof that each is in a separate packet or that native Studio supplies them in this order. A server may already have loaded a place and applied edits since that place was saved. If it supplies the resulting current world, the receiver can reconstruct that view; it does **not** automatically receive the original `.rbxl` bytes or a complete history of every edit. A real session may also omit or filter objects, assets, private state, source text, or other participants. Report those gaps instead of calling the result complete just because a marker arrived.

A robust reconstruction strategy is:

1. Record the session/build/schema identity; distinguish the schema's *descriptions* from actual records with values. The parsed prefix of `0x97` in the studied game path describes classes; it is not itself a decoded participant list, and its remaining tail is opaque.
2. Decode and stage every supported initial object record, including non-place session objects such as `Player` or `Collaborator` **if actually present**. Preserve network references, property presence, values and parent relations; resolve forward references without guessing.
3. Track incremental changes arriving while the baseline is in flight. If you control the server, take an internally consistent revision R and subscribe atomically before sending it; then replay changes after R in order. A receiver of a native server must use its actually observed ordering/barrier and must not assume Studio transmits a revision number named R.
4. Keep session presence separate from the persistable place, while letting them share references where the actual protocol establishes such a link. `Collaborator` properties in the pinned Studio API have `CanSave=false`; they are valuable for the *live session*, not evidence that the original place file stores connected users.
5. Only export as a complete place when the evidence supports completeness. `.rbxl` source, live script buffers, assets/terrain, unsupported values, filtered objects, and changes concurrent with capture may need additional work (§12 and §18).

In the **Boblox game** reference, an initial instance record may occur in an inflated `0x83/0x16` batch; the game parser can also receive live `0x02` spawns and `0x03` updates. §10.7 shows a tested *synthetic* `Collaborator` record and updates for that game schema. Their presence and wire encoding in an unmodified Studio Team Create session remain to be verified. The local Studio creation, status and removal evidence is covered separately in §11.8.



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

### 8.6 Synthetic player creation and update

**[DESIGN + GAME]** This example answers “where can the first player information and later updates be?” with concrete bytes for the **public game schema**, not a captured Studio join. It uses the `50 d3` schema in §2.5, invents participant `Alex` with user ID `42`, and assumes reference `(1,200)` is an already-existing `Players`-like parent. Neither the instance IDs nor this parent association are captured facts.

First, a generic `0x02` spawn whose class happens to be `Player`:

```text
83                           logical replication chain
02                           generic spawn item
01 c9 00 00 00               self reference: lo=1, id=201 (invented)
7c 01                        class index 380 = Player, u16 LE
18 04 41 6c 65 78            slot 24, DisplayName, tag 2: string "Alex"
31 54                        slot 49, UserId, tag 44: zigzag(42) = 84 = 0x54
39 04 41 6c 65 78            slot 57, Name, tag 49: literal "Alex"
ff                           end of property loop A
ff                           end of empty property loop B
01 c8 00 00 00               parent reference: lo=1, id=200 (invented)
00                           end of chain
```

One line: `83 02 01 c9 00 00 00 7c 01 18 04 41 6c 65 78 31 54 39 04 41 6c 65 78 ff ff 01 c8 00 00 00 00`.

Later, a hypothetical display-name change for the **same reference** uses a `0x03` item. In this schema `Player.DisplayName` has global ID 6022 = `0x1786` and is encoded as `86 17`:

```text
83                           logical replication chain
03                           generic property-update item
01 c9 00 00 00               target reference: lo=1, id=201
86 17                        Player.DisplayName global property ID 6022
05 41 6c 65 78 32            tag 2 string: "Alex2"
00                           end of chain
```

One line: `83 03 01 c9 00 00 00 86 17 05 41 6c 65 78 32 00`.

**Offline check:** The pinned Boblox `walk83ItemsEx` accepted both synthetic messages against `run/join97_new_frame.bin`, with complete walks of 31/31 and 16/16 bytes. It identified the first as one Player spawn with three properties and the second as one update to entity 201/property 6022. This proves only compatibility with that partial **game parser**. The parent was not created, no game server or Studio processed these bytes, and this test does not validate permissions, object lifecycle, event delivery, native bootstrap, or whether a Team Create client would accept the record.


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

These are registration strings/argument labels in the binary. Later descriptor and call-path analysis establishes some C++/reflection argument types and a direct `SendOperation` JSON dispatch (§11.7.3). They are **not** verified packet IDs, on-wire encodings, sender/receiver directions, authorization rules, or a complete event sequence. The presence of `OpenedWithSource` is not proof that a particular `0x07` game event transmits source or that every client is authorized to receive it. The hash algorithm and what the hash covers are not established.

### 10.3 An observed nested operation shape

**[STUDIO STATIC + SYNTHETIC EXAMPLE]** The pinned binary's operation writer/reader supports a nested JSON structure with the fields below. A later mocked execution also linked this logical shape to `SendOperation` dispatch (§11.7.3). This example was invented to illustrate the shape, not captured from a Studio session and not a complete network message:

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

### 10.6 Text cursors, collaborator selections, and 3D viewport presence

**[DOCUMENTED BEHAVIOR]** Studio shows each collaborator's colored text cursor in Live Scripting; code selections and selected 3D objects can also be highlighted in each collaborator's color. Studio can navigate to another collaborator's camera position or script context. These are distinct from the text stored in `Script.Source` and from persistent changes to parts of the place. See Roblox's [collaboration documentation](https://create.roblox.com/docs/projects/collaboration) and [collaborator-location announcement](https://devforum.roblox.com/t/jump-to-your-collaborator%E2%80%99s-location-in-team-create/2618317).

A useful distinction:

| State | Meaning | Recommended treatment in your server |
|---|---|---|
| Script text | Actual content; may change as users type and may need persistence. | Maintain document state and handle concurrent operations. `Script.Source` alone is not guaranteed to reflect the open editor immediately. |
| Text cursor and selection | Where a particular user is typing and what range they selected. A cursor can move even when text is unchanged. | Treat as per-user presence associated with a document and document version; it is not script source. |
| 3D viewport/camera location | Where a collaborator is viewing or working in Studio; other users may see a location indicator or use “Join.” | Treat as session presence, not automatically as a game character or persistent place object. |
| 3D object selection | Which instance(s) a collaborator selected or is manipulating. | Track separately from actual changes to the selected instance's properties. |

These are **logical distinctions**, not evidence of separate physical connections. Roblox may transport presence via instance properties, events, another mechanism, or a combination; the native encoding and initialization are not recovered here.

#### Concrete Studio API-dump clues

**[STUDIO STATIC — metadata, not captured traffic]** The official API dump for pinned Studio `0.745.0.7451137` lists a non-creatable `Collaborator` class with hidden properties including:

| Descriptor | Declared API type |
|---|---|
| `CFrame` | `CFrame`, categorized as Camera |
| `CollaboratorColor3` | `Color3` |
| `CurDocGUID` | `string` |
| `CurScriptLineNumber` | `int` |
| `UserId`, `Username` | `int64`, `string` |
| `IsIdle`, `Status` | `bool`, `CollaboratorStatus` |

In that dump, these properties have `CanLoad=false` and `CanSave=false`; `CollaboratorColor` is additionally listed as deprecated. This is useful evidence for **where to investigate presence**, and for not assuming these hidden fields are saved as ordinary place properties. Their names do **not** prove exact network messages, identity mapping, directions, update rates, or whether `CurScriptLineNumber` is the caret line rather than a viewport/context value. A line number alone would not describe an arbitrary character position or selected range.

The same dump lists `CollaboratorsService` functions/events such as `GetCollaboratorsList`, `RequestFlyToCollaborator`, `CollaboratorInstanceCreatedSignal`, `CollaboratorInstanceDestroyedSignal`, `CollaboratorStatusUpdatedSignal`, and `CollaboratorIdleUpdate`. These members are marked `RobloxScriptSecurity`: they are Studio API descriptors, not a documented public network protocol. Do not treat their names as opcodes or assume an external Rust server can call them. The dump also lists `CollaboratorStatus` names `None`, `Editing3D`, `Scripting`, and `PrivateScripting`; these enum labels are not proven wire values for a Team Create session.

#### How cursor positions interact with edits

**[DESIGN]** A custom collaboration service normally needs the user/document identity, caret position, selection anchor, a document version, and the user's color. When text changes, update or transform cursor/anchor positions to refer to the resulting document. For example, in an invented zero-based character coordinate system, if another user inserts three ASCII characters before a caret at position 5, that caret might move to position 8. Equal-position insertions, deletions across a selection, Unicode and pending local operations require precise rules. Do not adopt this invented rule or coordinate system as Studio's native one.

For reference, Roblox's **local plugin API** `ScriptDocument:GetSelection()` returns cursor line/character and anchor line/character; when there is no selection they coincide. `GetViewport()` describes the local editor's visible lines. The documented `ScriptDocument` conventions use 1-based lines and 1-based UTF-8-byte columns. These APIs describe the local editor proxy—not the format of a remote collaborator's wire messages. `ScriptDocument` is tagged `NotReplicated` in the pinned API dump; its documented internal editor-proxy synchronization is not evidence that the `ScriptDocument` instance itself is sent across Team Create.

#### Suggested presence model and lifecycle

**[DESIGN — not a Roblox packet or schema]** A server could keep an in-memory map per active participant containing:

```text
user identity and display color
active/idle state
3D camera or viewport pose, if sharing is permitted
current document identity, caret, selection anchor and document revision
selected instance identities, if sharing is permitted
```

Initialize a newly joined user with currently visible collaborators; relay later updates to authorized participants; clear the relevant marker when someone closes a document or leaves; and expire stale state on disconnect. Coalesce frequent camera/caret updates and bound per-client queues. Avoid exposing private script identity or selection to users not authorized to see it. On reconnect, replace the old presence epoch rather than leaving duplicate or stale markers.

A pose can be position plus orientation; the rendered avatar/camera marker is a UI representation of presence, not necessarily a regular persistent `Model`. The details of which fields are required, how a client creates/removes the UI, and whether presence is represented by `Collaborator` instances or another native mechanism require authorized observation. Snapshotting `.rbxl` state does not, by itself, restore the currently connected users' transient cursors and camera positions.

**Verification plan:** in an authorized two-Studio session, separately (1) move a text caret without typing, (2) select text, (3) scroll the editor without moving the caret, (4) move the 3D camera without moving any part, (5) select an object without changing it, and (6) leave/rejoin. Compare the resulting logical messages and client-visible state; check Unicode, document versions, privacy, and whether the reported `Collaborator` properties change. These experiments were not performed here.

### 10.7 Worked Collaborator baseline and updates

The `Collaborator` descriptors are a particularly useful lead for the initial synchronization the user described. The goal is to keep one **current presence state per observed collaborator** and then merge only the fields that later change. The examples in this section use Boblox's public **game** schema and synthetic data, not a captured Team Create session.

#### What each field can contribute

| Descriptor | Declared type | Useful interpretation / qualification |
|---|---|---|
| `UserId` | `int64` | Candidate account identity. Keep separate from the network instance reference and peer ID; bind authority to a validated session rather than trusting a received property. |
| `Username` | `string` | Candidate name for presence UI. Names may change or collide; do not use as the primary object identity. |
| `CollaboratorColor3` | `Color3` | Candidate color for that user's UI decorations. The game schema reads three f32 components (tag 19); actual Studio delivery is unverified. |
| `CFrame` | `CFrame` | Camera-related transform according to the pinned Studio API category. Useful for investigating viewport presence; the exact use and delivery direction are unverified. |
| `CurDocGUID` | `string` | Candidate current document identifier. Preserve it even if the referenced document has not arrived, but do not assume it is a path, a `Script.Source` value, or a proven wire GUID format. |
| `CurScriptLineNumber` | `int` | Current script-related line/context. Its name alone does not establish whether it represents the caret, viewport, or another line; it does not provide a column or selection range. |
| `IsIdle` | `bool` | Candidate idle flag; independent from status and document/source content. |
| `Status` | `CollaboratorStatus` | Candidate editing status. The pinned API dump declares `None=0`, `Editing3D=1`, `Scripting=2`, `PrivateScripting=3`. Preserve unknown enum values; these API values do not by themselves prove the native wire mapping or permissions. |

The `50 d3` game schema's local slots, tags and global property IDs are listed in §2.5. `Collaborator` is class index **141** (`8d 00` as u16 LE) in that one schema. The object references and property values below are invented; the class index comes from that public schema. In particular, the parent in this synthetic record is invented; §11.8 separately identifies a service-parenting call in one Studio-local creation path, not a proven network record.

#### A. One initial record inside a hypothetical game batch

**[SYNTHETIC + GAME]** This is the *decompressed record*, **not a complete `0x83/0x16` batch or a Studio packet**. It has the game snapshot shape `[self ref][class][property loop][ff][parent ref]`. The example represents user 42, username `Alex`, color `(0.25, 0.5, 1.0)`, a document identifier `doc-1`, line 12, idle false, status numeric value 2, and a transform at `(10, 4, -2)` with identity rotation according to Boblox's game CFrame table. `doc-1` is an illustrative string, not a validated Studio GUID.

```text
01 c9 00 00 00                    self reference: (lo=1, id=201)
8d 00                             class 141: Collaborator
00 00 00 20 41 00 00 80 40
   00 00 00 c0 02                 slot 0: CFrame position (10,4,-2), kind 2
02 00 00 80 3e 00 00 00 3f
   00 00 80 3f                    slot 2: Color3 (0.25,0.5,1.0)
03 05 64 6f 63 2d 31              slot 3: CurDocGUID, string "doc-1"
04 18                             slot 4: CurScriptLineNumber, zigzag 12
05 00                             slot 5: IsIdle=false
06 02                             slot 6: Status numeric value 2
07 54                             slot 7: UserId, zigzag 42
08 04 41 6c 65 78                 slot 8: Username="Alex"
ff                                end of property loop
01 c8 00 00 00                    parent reference: (lo=1, id=200), invented
```

One line: `01 c9 00 00 00 8d 00 00 00 00 20 41 00 00 80 40 00 00 00 c0 02 02 00 00 80 3e 00 00 00 3f 00 00 80 3f 03 05 64 6f 63 2d 31 04 18 05 00 06 02 07 54 08 04 41 6c 65 78 ff 01 c8 00 00 00`.

For tag 27, the game reader consumes three little-endian position floats and a kind byte; a zero kind has an additional six compressed-rotation bytes, while nonzero kinds use its orientation table. Here kind `2` maps to identity in that game implementation. A parser accepting the bytes is not evidence that Studio accepts this record or that this particular `CFrame` is actually a collaborator camera pose.

#### B. Three later changes for the same reference

**[SYNTHETIC + GAME]** In the same `50 d3` schema, the global IDs are 2103 (`37 08`) for `CurScriptLineNumber`, 2104 (`38 08`) for `IsIdle`, and 2099 (`33 08`) for `CFrame`. A hypothetical `0x83` chain can carry three individual `0x03` items:

```text
83                                logical replication chain
03 01 c9 00 00 00 37 08 1e       line = 15: zigzag(15) = 30 = 0x1e
03 01 c9 00 00 00 38 08 01       IsIdle = true
03 01 c9 00 00 00 33 08          CFrame update, global ID 2099
   00 00 30 41 00 00 80 40
   00 00 00 c0 02                 position (11,4,-2), identity kind 2
00                                end of chain
```

One line: `83 03 01 c9 00 00 00 37 08 1e 03 01 c9 00 00 00 38 08 01 03 01 c9 00 00 00 33 08 00 00 30 41 00 00 80 40 00 00 00 c0 02 00`.

The account ID, username, color, document and status were not included in these changes: a stateful consumer retains their prior values rather than clearing them. This example intentionally changes an idle flag and a pose in the same chain to demonstrate independent fields, not to claim such a combination is typical.

**Offline verification:** Using the pinned public Boblox parser and `run/join97_new_frame.bin`, `parseJoinRecords` consumed the 61-byte synthetic initial record and identified one `Collaborator` with eight properties. `walk83ItemsEx` consumed all 41 bytes of the synthetic update chain and identified three updates, with the names and tags shown above. A separate synthetic `0x02` live spawn of the same fields consumed 65/65 bytes. These checks validate the **partial game-parser shapes only**; no actual Collaborator session, parent object, compressed batch trailer, Studio acceptance, authorization, or UI behavior was tested. In the game reader a live spawn has **two** `ff`-terminated property loops; the initial record above has **one**.

#### C. Candidate collaborator lifecycle signals in the game schema

The same `50 d3` schema also **declares** event descriptors on `CollaboratorsService`. Their global event IDs are derived from the concatenated event lists, not from the property table:

| Event descriptor | Global event ID in this game sample | Schema argument tags |
|---|---:|---|
| `CollaboratorIdleUpdate` | 1482 | 44, 9 |
| `CollaboratorInstanceCreatedSignal` | 1483 | 44 |
| `CollaboratorInstanceDestroyedSignal` | 1484 | 44 |
| `CollaboratorStatusUpdateRequestedSignal` | 1485 | 44, 7 |
| `CollaboratorStatusUpdatedSignal` | 1486 | 44, 7 |

The matching pinned Studio API names the created/destroyed argument `collaboratorId`; it does **not** establish whether it equals `UserId` or a network reference. A descriptor in the schema does not establish that its event was observed, is allowed in either direction, or is required for Studio initialization.

As another **synthetic game-parser example**, suppose an invented `CollaboratorsService` has network reference `(1,200)` and an event argument with numeric value 42:

```text
83                          logical chain
07                          generic event item
01 c8 00 00 00              target ref: (1,200), invented
cb 05                       global event ID 1483, little-endian
54                          tag-44 argument: zigzag(42)
00                          end of chain
```

One line: `83 07 01 c8 00 00 00 cb 05 54 00`.

The pinned game `walk83ItemsEx` consumed all 11 bytes and selected `CollaboratorsService.CollaboratorInstanceCreatedSignal` with one tag-44 argument. It also parsed independently fabricated status-updated and idle-update examples with the tags in the table. This is a **schema/parser test**, not a captured collaborator event and not permission to send these bytes to Studio.

#### D. How to integrate it into your Rust port

**[DESIGN]** Maintain two different indexes, scoped to a connection/session epoch:

```text
by_instance[(session_epoch, full_network_reference)]
    → class + partial presence fields + parent/reference resolution state

by_user[(session_epoch, user_id)]
    → zero or more candidate Player/Collaborator references
```

1. On an initial record or live creation, resolve its class and each member through the **received schema**. If its class is `Collaborator`, retain every supported field and mark omitted/unknown fields as unknown rather than inventing default values.
2. Decode and apply only the fields actually present. Preserve the full network reference, not just its u32 component. Keep unresolved parents and document links in bounded pending structures.
3. On a later update, resolve the global property ID through that session's schema and applicable overrides; validate that it is meaningful for the target object/class, then update that one field. If an update precedes its creation, hold or reject it according to verified ordering rules, with strict memory/time limits.
4. Use a validated account/session mapping to correlate a `Player`, a `Collaborator`, and a connection when the evidence supports it. A shared `UserId` is a useful candidate correlation, not proof of one-to-one object identity; usernames and peer IDs are not substitutes. Multiple connections or reconnects may need distinct entries.
5. Keep presence transient. Refresh or remove it on the **actual verified** leave/destruction/disconnect path; Boblox's game runtime currently parses `0x01` but does not apply it as deletion, so do not declare that item to be Studio's collaborator-removal rule.
6. Keep the document's actual text and operations separate from the presence fields. `CurDocGUID` and a line number cannot recover script source, selection ranges, editor revisions, or all collaborative operations. Respect permissions, `PrivateScripting`, and data minimization.

This design is useful immediately in a test harness, but a server for unmodified Studio still needs the target-build Team Create schema, actual instance/service and parent arrangement, which fields/events are sent in each direction, initialization and removal semantics, and the complete native envelopes.



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
presence/        transient collaborator cursors, camera poses and selections
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

### 11.6 How my edits reach the server and other collaborators

This section answers the collaborator's question: **“I am the person editing. How do I tell the server to sync my place edits, camera and other changes to everyone else?”**

#### Who sends what?

* **If your editing client is unmodified Studio:** once it is connected to a properly functioning Team Create session and you have Edit permission, you perform normal edits in Studio. Studio's own client is responsible for generating its native collaboration messages. You do not call `Collaborator` as a generic “send my `.rbxl`” command.
* **If your editing client is your Rust/Boblox port:** receiving and decoding the initial replication is not enough. Your port also needs the **client-to-server native writers**, the correct session identity, ordering and any required acknowledgements. The pinned Boblox research describes its game receiver but says upstream owned writes were not implemented; it is not a complete Team Create edit sender.
* **If you are implementing the Rust server:** it must decode the actual incoming Studio edits, authorize and apply them, then generate valid native updates for other connected Studio clients. The Rust project delivered with this guide does **not** yet implement that end-to-end native adapter.

The `Collaborator` class is useful evidence for **who is present and where they are working**. It is marked `NotCreatable` and its relevant fields are hidden in the pinned Studio API; `CollaboratorsService` methods/signals include `RobloxScriptSecurity` members. These descriptors are not a documented general-purpose public API for commanding a server to import a file or broadcast changes, and their existence does not establish what Studio transmits when a field changes.

#### A. Put the `.rbxl` on the authoritative side

An `.rbxl` sitting on one collaborator's computer is not automatically visible to the server. For a **custom server you control**, a practical bootstrap is for the server operator to import the file through a separate trusted setup mechanism, such as loading a local server-side file with `rbx_binary::from_reader`. That is an administrative import, not a native Team Create packet. The server then holds an authoritative in-memory place and must expose the initial state through whatever **verified native bootstrap** Studio requires. The file need not be transmitted verbatim.

If a collaboration session is already active, replacing its whole place with a local file is a separate, potentially destructive operation: authorize it, back up or preserve current work, define conflict policy, and resynchronize participants using an actually supported workflow. Do not silently overwrite the existing server state, and do not assume unmodified Studio exposes an upload-whole-place command for your custom server.

For Roblox's hosted service, the corresponding user workflow is to have an authorized shared experience, open that experience in a Team Create session, and edit it with the necessary permissions; a separately opened local file is not by itself the shared session. See [Roblox's collaboration documentation](https://create.roblox.com/docs/projects/collaboration). Live Scripting and Drafts can behave differently, so do not assume every local editor buffer is already the shared source.

#### B. Normal place-instance edit: client → server → clients

**[DESIGN — internal commands, not a native wire format]** Suppose Alice moves an existing Part, and Bob is also editing the same place:

```text
Alice's Studio                Rust collaboration server                 Bob's Studio
──────────────                ─────────────────────────                 ────────────
Alice moves Part       ──native client edit message──▶  decode message
                                                        derive Alice from session
                                                        map target reference
                                                        authorize + validate
                                                        commit Part.CFrame in world
                                                        assign internal ordering
                                                        encode Bob's native update ──▶  apply update
                              respond/ack to Alice as the *verified protocol* requires
```

The server-side operation can be represented internally as:

```text
SetInstanceProperty {
    actor: authenticated_session_user,
    target: server_internal_object_id,
    property: CFrame,
    value: position_and_rotation
}
```

This is **not** JSON or bytes to send to Studio. The implementation steps are:

1. Receive a **complete, decrypted, transport-reassembled client→server application message**. Use the actual build/session codec to decode its item, target, property and value. Do not confuse packet-layer flags with application item IDs.
2. Derive the actor from the authenticated connection. Check Edit permission and any per-object/property restrictions; verify the target exists, the class supports the property, the CFrame is valid and finite, and the change is permitted. Never authorize based only on a `UserId` claimed in a payload.
3. Resolve the incoming network reference to the server's authoritative object. A `.rbxl`/DOM referent and a network reference are separate identities. Keep per-session mappings until you have verified the native namespace rules.
4. Serialize world edits in a consistent order, mutate the authoritative model once, and journal/checkpoint persistent changes according to your storage policy. If a client is still receiving an initial snapshot, it must subsequently receive the relevant ordered changes without gaps or duplicates.
5. Encode the committed update for each authorized recipient using that recipient's verified schema and reference mapping. Do not blindly forward Alice's raw packet when IDs, schema or visibility can differ between connections. If safe forwarding is later proven for a particular protocol, it can be an optimization.
6. Send the correct acknowledgement or echo to Alice **only as the actual protocol requires**. Alice may have applied an optimistic local edit already; an unexamined echo can apply it twice. Handle disconnect, retry and resynchronization explicitly.

Creating an instance, reparenting, deletion, attributes and references each need their own native decoder and validation. Reparenting must not create cycles; a deleted object must not be accidentally resurrected by an older queued update. `.rbxl` checkpoints are not a substitute for a correctly ordered live change stream.

#### C. My camera is presence, not a Part edit

A Part's `CFrame` changes the shared place object. Your **Studio viewport camera** describes where *you* are looking; it is normally transient per-collaborator presence, not a request to move every other user's viewport or persist a camera into the place.

The pinned Studio API marks `Camera` and `Workspace.CurrentCamera` `NotReplicated` and lists hidden `Collaborator.CFrame` under a Camera category. This makes `Collaborator.CFrame` a useful investigation lead, but does **not** prove its wire encoding, whether Studio sends it directly to the server, or how another client receives it. Roblox's documented collaborator **Join** feature moves the local user to the other collaborator's view; the view is not locked afterward. See the [collaborator-location announcement](https://devforum.roblox.com/t/jump-to-your-collaborator%E2%80%99s-location-in-team-create/2618317).

**[DESIGN]** If the verified native protocol delivers a user's camera pose, a server can associate it with the authenticated session, update that collaborator's transient presence, and relay only to authorized participants. A conceptual server-side update is `SetPresencePose { actor: session_user, pose }`—not a native message. Never allow Alice to update Bob's presence simply by supplying Bob's user ID. Throttle/coalesce high-frequency pose updates, avoid storing them as `.rbxl` edits, and clear stale presence on disconnect. Selection, text caret and document location require their own decoded data and privacy rules; a camera `CFrame` alone cannot describe them.

#### D. Script typing is not necessarily a property replacement

For Live Scripting, the server must handle the native document-open/source state and edit operations, their order, acknowledgements and concurrency. The pinned Studio binary contains `BeginEdit`, `SendOperation`, `SendAtomicOperation` and an inner operation JSON shape (§10), but those are not complete verified client→server packets. `CurDocGUID` and `CurScriptLineNumber` are presence clues; they cannot replace the actual source text or operations. Do not send the synthetic nested JSON from §10.3 to Studio or a Roblox server as if it were the native envelope.

#### E. What we know—and what still blocks an actual send

**[STUDIO STATIC]** In the pinned Windows Studio 0.745 binary, one `ChangePropertyItem` path writes item byte `0x03`, calls a reference-related helper, writes a two-byte metadata field, and invokes further serializers. Section 11.7 now traces the queue, writer and selected reference behavior in more detail. **[GAME]** Boblox's game receiver parses its `0x03` as a reference, global property ID and typed value. This is a promising overlap to investigate, not proof of a Team Create client→server message, full `CFrame` layout, identity mapping, authorization, or ack/echo semantics.

The synthetic game `0x03` examples in §§7 and 10.7 were accepted by the **game parser only**. They do not show that an unmodified Studio client sends those bytes or a Team Create server accepts them. To support the user's native target, the remaining blocker is the build-specific inbound/outbound Team Create codec and lifecycle—not a missing `Collaborator` method.

**Future verification, if live Studio access becomes available:** compare an authorized two-client session with an idle control; make exactly one change at a time: rename a Part, move a Part, create/reparent/delete an instance, move only the editor camera, move only a caret, and type one character. Record direction, logical message boundaries, schema fingerprint and resulting behavior on the server and second client, plus any ack/echo. Redact credentials, source and private user data. The user previously selected offline research, so none of these Studio captures has been made here; static analysis can identify candidates but cannot prove interoperability.


### 11.7 Decompilation: how Studio constructs outgoing changes

This section answers the send-path question using the actual pinned Windows Studio executable, not only the Boblox game parser. It is the most direct evidence in this guide about what Studio constructs. **It is still not an observed Team Create network session.** Addresses are virtual addresses in Windows Studio `0.745.0.7451137`, deployment `version-7062aec9dbe74b14`, executable SHA-256 `781b36ed6fb62e61292b9e6a64327280c7da12b00f3ea8b2c91a3f4834f860ec`. They are not stable identifiers across releases.

| User action | What the pinned code establishes | What it does not establish |
|---|---|---|
| Change an instance property | A `ChangePropertyItem` queue and writer path emits a selected item header, reference and a two-byte metadata field, then calls value-related code. | Exact Team Create envelope, every typed value, runtime subscription for each property, authorization or echo. |
| Move the editor camera / change collaborator status | `Collaborator` fields have real setters and change notifications. `CFrame` is copied as an **in-memory** value. | Whether camera motion reaches that setter in every case, network direction, actual bytes or automatic replication. |
| Move a script caret | An update helper builds the `cursors` JSON structure and passes it as a string to a named remote-event descriptor. A separate broadcast helper builds `userCursors`. | Coordinate origin/units, timestamp interpretation, network direction or complete frame. |
| Type in a script | A helper serializes an operation into JSON and dispatches it with `SendOperation(id, op)`; the relevant runtime descriptor has an `int64` plus a string argument. | Exact wire encoding, source/open and ack lifecycle, position units, complete OT and end-to-end routing. |

The words **send** and **remote event** below describe what the inspected code and descriptors do internally. The address chain does not by itself prove which network peer sends a particular event or that a game-schema record is its final packet.

#### 11.7.1 Property changes: from a queued item to a partial byte sequence

**[STUDIO STATIC]** In the pinned executable, the RTTI names `Network::Replicator::ChangePropertyItem`. Its constructor is `0x14612fa70`. One path at `0x145fc5c00` looks up class/property metadata (`0x145f0eb70`, `0x145fbf810`), selects an entry, constructs an item and queues it through `0x14601ef40`. Another caller at `0x145f6bcf0` also reaches the constructor. A vtable entry points to `0x146130a00`, whose path reaches the writer `0x14612f820`.

The relevant writer path is, schematically:

```text
ChangePropertyItem / selected schema metadata
   └─ 0x14612f820
       ├─ item-code helper 0x14601f0d0(..., 3)
       ├─ reference writer 0x1460c6210(..., instance)
       ├─ write two raw bytes from selected metadata, 0x142a28f20
       ├─ virtual callback at replicator vtable +0x410
       └─ value-related serialization 0x145f455d0 / 0x145fa34a0
```

This is **not** enough to say the final wire record is always `03 + reference + property ID + value`: callbacks and branches matter, the containing message is unknown, and the two-byte field has not been independently established as a Team Create property ID. The path does, however, show why the Boblox game's similar-looking `0x03` record is a useful comparison.

**[STUDIO STATIC / SYNTHETIC EMULATION]** For the reference writer examined here:

1. If the object pointer is null, or the low 32-bit scope value is `0xffffffff`, it writes unsigned varint zero and does not write the object-number word.
2. Otherwise, it writes the low 32-bit scope as an unsigned base-128 varint. If that scope is zero it substitutes a default scope from the replicator.
3. It then writes the high 32-bit object number as four raw little-endian bytes.

This is how this particular helper behaves; the precise native identity/namespace and collision rules are still unknown. A local `.rbxl` referent must not be treated as that network ID without an explicit mapping.

The following are **partial header outputs**, obtained by running the selected native writer/reference/varint machine code in Unicorn with synthetic objects and selected allocation, logging, memory-copy and callback side effects mocked. The test stops before the typed property value. `0x1234` is an invented metadata field, not a discovered property ID:

```text
Synthetic input                         Selected header bytes
scope 1, object 201, field 0x1234       03 01 c9 00 00 00 34 12
scope 300, object 0x11223344            03 ac 02 44 33 22 11 34 12
  same invented field 0x1234
scope 0, default 5, object 201          03 05 c9 00 00 00 34 12
null object, field 0x1234               03 00 34 12
scope 0xffffffff, field 0x1234          03 00 34 12
```

Read the first line as item byte `03`; scope `01`; object number `c9 00 00 00`; invented field `34 12`. `ac 02` is varint 300. These bytes are **not a complete packet, not a captured Studio edit, and not a valid example to transmit**. They isolate the behavior of selected functions in a controlled test. The emulator's mocked virtual callback in particular cannot establish whether real callbacks add data or alter state.

#### 11.7.2 Events: how script/presence payloads might enter generic replication

**[STUDIO STATIC]** There is also an `EventInvocationItem` constructor at `0x1461312c0`, with a queueing path through `0x145f865d0` and `0x14601ef40`. A vtable entry leads `0x146132870 → 0x1461329c0`. In its normal, non-cached branch, the latter writes byte `07`, calls the same reference writer, writes a raw two-byte metadata field, and iterates arguments through `0x145f47420`. It has an alternative cached/pre-serialized branch. Generic signal/event dispatch code includes `0x142addf70` and `0x142a674b0`.

This narrows the investigation: **the pinned Studio has a generic event item with a superficially game-like `07` prefix.** It does not establish that a particular `SendOperation` or cursor descriptor is connected to that network item in a Team Create session. In particular, do not construct `07` plus invented IDs and JSON and call it a working Studio message. The runtime listener connections, descriptor/schema lookup, argument encodings, event direction, permissions and containing frame must be established.

#### 11.7.3 Actual `SendOperation` dispatch and operation JSON

The prior section showed the JSON fields. The additional decompilation links an operation writer **directly to a specific event descriptor**:

```text
0x145c51f30(operation object)
   ├─ 0x145c81b00
   │   ├─ 0x145c81050: write payload.atoms[]
   │   └─ 0x145c818d0: write timestamps.local / timestamps.remote
   └─ 0x145c64be0 → 0x142addf70
        descriptor read from 0x14ca00618 → 0x14d8292f0
        registered at 0x1406284b0 as SendOperation(id, op)
```

The descriptor's RTTI identifies a `LiveScriptingService` remote-event signature equivalent to **signed 64-bit integer plus `std::string` at the C++/reflection level**. `id` is an event argument, not a proven user ID, revision or on-wire integer representation. The string is the serialized operation passed through this path; the network argument encoding and complete envelope remain unresolved.

**[STUDIO STATIC / SYNTHETIC EMULATION]** The machine-code path `0x145c51f30 → 0x145c81b00 → 0x145c81050 / 0x145c818d0` was executed on a synthetic operation. The JSON library and downstream event dispatch were mocked. The mock observed one `SendOperation` descriptor dispatch with invented ID `42` and the following logical string contents:

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

Those values and the ID are invented. The emulation confirms the tested code's field/control-flow relationship under mocked helpers, **not exact native JSON byte formatting**, a captured event or a working client/server exchange. Numeric type 0 behaves like an insertion in one isolated transform, but its complete meaning and the position's byte/character/line unit are not established (§10.4).

There is another promising connection: a `LiveScriptingDMClient` path at `0x145b280a0` logs “Sending edit for ID”, iterates operation-like strings and calls an object's virtual slot `+0x58`. The RTTI-identified `LiveScriptingService` vtable at `0x1494dc1f8` has `0x145c65970` at that same slot. `0x145c65970` copies the operation string and dispatches through `0x145c64b10` / `0x145c64be0` using the same `SendOperation` descriptor. This is strong static consistency, **not proof that the DM client's actual dynamic object always uses that vtable**; the virtual target in a live session has not been observed.

Related names such as `SendAtomicOperation(requestId, guid, op, hash)` and `OpenedWithSource` are registered too. Their full sequence, string formats, hash algorithm, document-open behavior, acknowledgement and recovery semantics are still unknown. Do not treat a local `Script.Source` property update as a replacement for that missing editor protocol.

#### 11.7.4 Script cursor/selection presence: two different JSON objects

**[STUDIO STATIC]** The update helper `0x145c6a4d0` calls `0x145c6f000` to serialize cursor data and passes two strings to `0x145c6e7b0`. The descriptor pointer `0x14ca005a8 → 0x14d8286d0` is registered at `0x140627550` as:

```text
UpdateClientRemoteCursorPosition(script, remoteCursorPosition)
```

RTTI for the relevant descriptor uses two C++ strings. An analogous URI variant (`0x145c6eec0`) uses a different descriptor but the same cursor JSON builder. A two-cursor synthetic input, executed through the real JSON-builder machine code with JSON primitives mocked, produces this **logical object** for the second argument:

```json
{
  "cursors": [
    {
      "positionLineIndex": 3,
      "positionCursorIndex": 5,
      "anchorLineIndex": 3,
      "anchorCursorIndex": 5
    },
    {
      "positionLineIndex": 4,
      "positionCursorIndex": 1,
      "anchorLineIndex": 4,
      "anchorCursorIndex": 2
    }
  ],
  "serializedTimestamps": "clock"
}
```

The `script` argument is a string at this reflection boundary; it is not yet proven to be a GUID, path or URI in this variant. `"clock"` is a deliberately arbitrary stand-in, **not** a valid timestamp specification. Position/anchor field meanings, bases, units and remote ordering still require verification; the plugin-facing `ScriptDocument` indexing rules are not proof of wire rules.

**A different helper serves an aggregated/broadcast-shaped event.** `0x145c6ecd0` calls `0x145c6f150` and uses descriptor `0x14ca00590 → 0x14d8284c0`, registered as `BroadcastRemoteCursorPositionsWithConnectionId(connectionId, remoteCursorPosition)`. RTTI identifies a signed 64-bit argument and a string at the reflection boundary. Selected real builders were separately emulated with mocked JSON primitives and invented values:

```json
{
  "userCursors": [
    {
      "ownerUserId": 123,
      "playerName": "Alice",
      "cursors": [
        {
          "positionLineIndex": 3,
          "positionCursorIndex": 5,
          "anchorLineIndex": 3,
          "anchorCursorIndex": 5
        }
      ],
      "maybeOudated": false
    }
  ],
  "timestamps": "clock"
}
```

The spelling **`maybeOudated` is exactly as found in this build**, even though it looks like a typo. The pinned JSON writer path treats that field as a Boolean. The singular update uses `serializedTimestamps`; the aggregated form uses `timestamps`. Both examples have invented data, mocked JSON functions and no native transport envelope. Descriptor names suggest roles, but **do not prove client→server or server→client direction**.

#### 11.7.5 Collaborator camera and state: observed setters, not a camera packet

The pinned `Collaborator.CFrame` registration associates the name with getter `0x140d68060` and setter `0x144e55470`. The setter checks for a changed value, copies 48 bytes of its **in-memory** representation, then calls the change-notification helper `0x142a67fa0`. That does **not** prove the network value consists of 48 raw bytes; the typed wire serializer may encode it differently.

The pinned binary also associates these names with setters that notify on change:

| `Collaborator` field | Setter address |
|---|---:|
| `CollaboratorColor3` | `0x144e55590` |
| `CurDocGUID` | `0x144e55770` |
| `CurScriptLineNumber` | `0x144e558e0` |
| `IsIdle` | `0x144e559c0` |
| `Status` | `0x144e55aa0` |
| `UserId` | `0x144e55c00` |
| `Username` | `0x144e55cd0` |

`CollaboratorsService` also registers `CollaboratorStatusUpdateRequestedSignal(collaboratorId, newStatus)`. These facts make the class useful for tracking camera/document/identity presence. They do not establish that a user's camera motion always reaches this setter, that its notification is connected to the queued item path in the target session, or that the service signal is sent in a particular direction. Nor do `CurDocGUID` or line number carry the document text.

#### 11.7.6 What to implement, and what not to assume

For your Rust server, a useful division of responsibilities is:

```text
Authenticated connection + native framing/schema/reference adapter
  ├─ world changes: resolve object → validate property/value → commit → relay
  ├─ presence: bind state to session actor → coalesce/relay → expire on leave
  └─ live document: open/source state → ordered operation processing → relay/ack
```

The adapter cannot yet be completed solely from these static observations. In a test decoder, only after the actual native framing and schema are known, the selected `03`/`07` writer paths are reasonable starting points for locating a reference, metadata and typed payload. Maintain build-specific tests; reject unknown or malformed data instead of guessing field IDs or consuming the rest of a message as a value. Do not assume DOM references, schema IDs, user IDs and document handles are interchangeable.

On every incoming edit, derive the actor from the authenticated session, check edit permission and object/document ownership, validate bounds and types, then apply once in a consistent order. Keep snapshot and subsequent changes consistent; use the **actual** protocol's echo, acknowledgment and resynchronization behavior when established. Presence should be ephemeral and throttled; source editing requires correct operation transforms and revision handling, not arbitrary full-text overwrites. These are server-design requirements, not a claim that the missing native protocol is implemented.

**Offline verification boundary:** selected native code was decompiled and executed under synthetic emulation, but no unmodified Studio session, encrypted capture, second client or end-to-end exchange was available. The evidence files and exact mocked boundaries are listed in §19. The missing framing, value codecs, event links/directions and document lifecycle are the next blocking facts, not a matter of guessing a packet from the JSON examples.


### 11.8 Verifying `Collaborator`: fields, identity, and lifecycle

**What was actually checked:** the official Windows Studio `0.745.0.7451137` API dump and the executable with SHA-256 `781b36ed6fb62e61292b9e6a64327280c7da12b00f3ea8b2c91a3f4834f860ec`. We traced class registration, methods, several local creation/update/removal paths and property setters. We also executed selected functions with synthetic state and mocked helpers. This is stronger evidence about **this build's local object model**, but it is **not a capture of Team Create traffic**. Later builds may differ.

#### 11.8.1 The nine class-specific fields

**[STUDIO API + STATIC]** The pinned API describes `Collaborator` as an `Instance` tagged `NotCreatable`. It has the following nine **own** properties; inherited properties such as `Name` and `Parent` are separate. All nine are tagged `Hidden`, have `CanLoad=false` and `CanSave=false`, and have `None` read/write security in the API dump. The serialization flags concern loading/saving; they **do not tell us whether a field is or is not replicated**.

| Field | Pinned API type | What the verification adds | What not to infer |
|---|---|---|---|
| `CFrame` | `CFrame` | Category is `Camera`; setter `0x144e55470` compares, copies an in-memory value and calls a property-notification helper on change. | A camera packet, direction, update frequency or 48-byte wire value. |
| `CollaboratorColor` | `int` | Exists but is explicitly tagged `Deprecated`. | That it has the same encoding or semantics as `CollaboratorColor3`, or that it must be sent. |
| `CollaboratorColor3` | `Color3` | Setter `0x144e55590`; one local creation path assigns a color. | That the chosen color is transmitted as three raw floats or initialized identically in every session. |
| `CurDocGUID` | `string` | Setter `0x144e55770`; the status path consults a lookup related to this property. | That it contains document text, or that its wire representation is a particular UUID/path format. |
| `CurScriptLineNumber` | `int` | Setter `0x144e558e0`; a selected synthetic test changed its local value to 15. | Whether the line describes caret or viewport, its indexing base, a column or a selection. |
| `IsIdle` | `bool` | Setter `0x144e559c0`; an idle helper also dispatches an `IsIdle`-related service signal. | That property updates and the service signal are identical or have a known network direction. |
| `Status` | `CollaboratorStatus` | Setter `0x144e55aa0`; a separate path selects a status and dispatches `CollaboratorStatusUpdatedSignal`. | That any claimed client status proves authorization or access to a private document. |
| `UserId` | `int64` | Setter `0x144e55c00`; in one creation path it is initialized from the same ID used for the created signal. | That it equals the network instance reference, connection ID, or every other event's ID. |
| `Username` | `string` | Setter `0x144e55cd0`; the same creation path supplies the username string to the `Instance.Name` setter. | That a name is unique, immutable, or a safe authority key. |

The pinned `CollaboratorStatus` enum is `None=0`, `Editing3D=1`, `Scripting=2`, `PrivateScripting=3`. These are API/reflection values. Preserve unknown numeric enum values in a build-tolerant reader; a numeric API value does not itself prove its wire encoding. Do not expose private document names or contents merely because a user has a particular status.

The complete extracted API entries are in [`collaborator-api-pinned.json`](roblox-team-create-rust/evidence/studio-outgoing/collaborator-api-pinned.json). The generic outgoing `0x03` path described in §11.7.1 and these setters are **separate observations**: the listener connecting a particular `Collaborator` setter to a particular network item has not been traced in a running Team Create session.

#### 11.8.2 A real local creation path: the useful ID relationship

**[STUDIO STATIC]** Function `0x144e503d0` takes an ID and username string and, on the examined path, performs these local calls in this order:

```text
create a Collaborator object
  UserId       ← input ID             (through the UserId descriptor's setter)
  Username     ← input username       (through the Username descriptor's setter)
  Color3       ← chosen local color   (CollaboratorColor3 setter)
  dispatch CollaboratorInstanceCreatedSignal(service, input ID)
  pass input username to Instance.Name setter
  pass service object to Instance parent-setting helper
```

The important, directly supported relationship is **this path initializes `Collaborator.UserId` with the same input ID passed to `CollaboratorInstanceCreatedSignal`**. We also traced the two generic helpers: the first reaches the `Instance.Name` setter registered with the `Name` descriptor; the second reaches a parent-management routine containing a “Setting parent” diagnostic. That is evidence of a service-parenting call in this path, not proof of the tree in every session. The local signal-dispatch call precedes the name and parent-setting calls here; it does **not** establish the order or timing of anything on the network.

`CollaboratorsService` is itself an `Instance` with `Service` and `NotCreatable` tags. Its child-validation virtual function at `0x144e52440` rejects an object failing its `Collaborator` type check with the diagnostic `Child of collaborators service must be a collaborator`. The child-removal callback `0x144e4e8a0` checks the child's type, obtains its `UserId` via the descriptor and performs an ID-based map lookup. These observations are local service behavior; they do not prove all objects arrive as replicated children or that there is only one object per user.

A separate removal-shaped function, `0x144e51740`, takes an ID, searches a service collection, calls an object-management helper when a matching entry is found, and calls the dispatcher for `CollaboratorInstanceDestroyedSignal` with that same input ID. We did not verify the eventual destruction, callbacks, delivery or all alternate paths. Do not derive a native destroy packet or universal event ordering from it.

Instruction traces: [creation and child checks](roblox-team-create-rust/evidence/studio-outgoing/collaborator-create-and-child-disassembly.txt), [name/parent registration and setter evidence](roblox-team-create-rust/evidence/studio-outgoing/collaborator-name-parent-evidence-disassembly.txt), [parent-management core](roblox-team-create-rust/evidence/studio-outgoing/collaborator-parent-core-disassembly.txt), and [removal/callers](roblox-team-create-rust/evidence/studio-outgoing/collaborator-removal-and-callers-disassembly.txt). Address-preserving [Ghidra pseudocode](roblox-team-create-rust/evidence/studio-outgoing/collaborator-lifecycle-pseudocode.txt) is supporting context only: incomplete external memory/types and removed-block warnings limit it.

#### 11.8.3 Service methods and signals are not automatically packets

The pinned API dump declares these `CollaboratorsService` members with `RobloxScriptSecurity`:

| Member | API-level signature | What is established |
|---|---|---|
| `GetCollaboratorsList` | `() → Instances` | Registered to local method `0x144e4dff0`; one inspected implementation iterates a service collection. It is not a network bootstrap specification. |
| `RequestFlyToCollaborator` | `(collaboratorId: int64) → null` | Registered to a local method; its name/signature do not establish a transport message or remote camera behavior. |
| `CollaboratorInstanceCreatedSignal` | `(collaboratorId: int64)` | The creation path above dispatches the input ID after initializing the object's ID and username. |
| `CollaboratorInstanceDestroyedSignal` | `(collaboratorId: int64)` | The inspected removal-shaped path dispatches its input ID after a successful lookup. |
| `CollaboratorStatusUpdatedSignal` | `(collaboratorId: int64, newStatus: CollaboratorStatus)` | The status path reads the object's `UserId` and stored `Status`, then dispatches those values. |
| `CollaboratorIdleUpdate` | `(collaboratorId: int64, isIdle: bool)` | The idle path sets an object's `IsIdle` and dispatches using its own ID/bool arguments. Equality between that ID argument and that object's `UserId` was **not** established for this path. |

`CollaboratorStatusUpdateRequestedSignal(collaboratorId, newStatus)` is also registered by the binary, but is **absent from the inspected API dump**. Its existence alone does not establish which peer may invoke it or its wire format. The dump includes other service members as well; see the complete API excerpt rather than treating this table as the entire service API. `RobloxScriptSecurity` is a restriction on those API members; it is not a license to invoke them from arbitrary plugin code.

In the examined status-update helper `0x144e51a80`, input `1` selects status `1`; inputs `2` or `3` go through a `CurDocGUID`-related lookup that can select `2` or `3`; other inputs select `0`. The helper calls the status setter, reads the stored status and obtains that object's `UserId`, then calls a service dispatcher with `(UserId, stored status)`. This is one local path, not a specification of every valid status transition, permission check or network message.

**Important distinction:** a setter's change notification and a service signal are different mechanisms. In the status helper above, the service dispatcher is called after the setter even if the selected status already equals the current status. You cannot infer one service signal per property notification, or deduplicate all events based only on the current property value.

#### 11.8.4 What was executed, with what mocked

**[STUDIO EMULATED]** Six actual setter functions from the pinned executable were run under Unicorn using a synthetic object, a disabled optional-instrumentation flag, and fake external hooks. Each test set a changed value and then repeated the identical value:

| Test | Result in this controlled test |
|---|---|
| `IsIdle`: `false → true → true` | Stored `true`; one mocked property-notification call for the change, none for the identical repeat. |
| `Status`: `0 → 2 → 2` | Stored `2`; one mocked notification, none for the repeat. |
| `UserId`: `0 → 4294967301 → 4294967301` | Retained the value above `2³²`; one mocked notification, none for the repeat. |
| `CurScriptLineNumber`: `0 → 15 → 15` | Stored `15`; one mocked notification, none for the repeat. |
| `CollaboratorColor3`: chosen finite RGB value repeated | Copied the selected in-memory value; one mocked notification, none for the identical repeat. |
| `CFrame`: chosen 48-byte in-memory value repeated | Copied those 48 bytes; one mocked notification, none for the identical repeat. The rotation-equality helper was replaced by an exact-byte mock. |

We also ran the real status-update wrapper with status `1` twice after an initial local status of `2`. The reflected `UserId` getter, notification helper and downstream service dispatcher were mocked: the test observed **one notification call but two dispatcher calls**, both carrying synthetic `UserId=4294967301` and status `1`. A stack-cookie checker and pre-set guard were also mocked. This is an isolated check of those code paths—not a live Studio process, actual listener delivery, serialization, transport, permission test or interoperability result. It does not establish general floating-point/NaN equality behavior or a `CFrame` wire size.

The [test output](roblox-team-create-rust/evidence/studio-outgoing/collaborator-setter-emulation.txt), [readable harness](roblox-team-create-rust/evidence/studio-outgoing/emulate_collaborator_setters.harness.txt), [setter disassembly](roblox-team-create-rust/evidence/studio-outgoing/collaborator-setters-disassembly.txt), and [evidence README](roblox-team-create-rust/evidence/studio-outgoing/README.md) identify the inputs, mocks and reproduction requirements.

#### 11.8.5 Applying this to your Rust port

**[DESIGN, not a native message format]** Once your port has actually decoded a field or service signal from the correct current schema, keep these identities distinct:

```text
(session epoch, network object reference) → observed Collaborator state
(session epoch, UserId)                   → zero, one or multiple object references
connection/session                        → independently authenticated actor
```

Use a signed 64-bit representation for the API-level `UserId`; the test deliberately used `4294967301` to catch accidental 32-bit truncation. Keep the entire network reference as an opaque key until its real namespace/lifetime is established. Maintain a separate user-to-object index that can handle missing IDs, duplicate references, reconnects or later corrections; do not collapse actors merely because names match. The service-signal ID and `UserId` can be correlated in the **specific observed created/status paths**, not universally equated with the reference or peer ID.

For an illustrative **decoded local state**—not bytes to send—you could have object `R` with `UserId=42`, `Username="Alex"`, `Status=Scripting`, `CurDocGUID="doc-1"` and `IsIdle=false`. Later, apply only a verified `IsIdle=true` change to `R`; preserve the other fields. If a decoded status signal for ID 42 also arrives, route it separately and reconcile it with object state under the actual protocol's ordering rules. Here `R`, `42`, and `doc-1` are invented examples, not established native references or GUIDs.

For your own server's internal design, bind writes to the authenticated actor, validate edit permissions, bound presence/update rates, and use a bounded pending queue for an event referring to an as-yet-unknown object. Keep presence ephemeral and outside your persistable place model unless your actual application needs an explicit export. These are design recommendations, not proven native rules. **A complete Studio-compatible implementation still requires actual schema IDs and typed codecs, object creation/removal framing, property/event subscriptions, direction, authorization, bootstrap and ordering.**


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
| Joined, but no self or collaborators appear | Independently check authenticated-user binding, any actual peer-init, class/instance creation, presence initialization, and bootstrap/ack handling | That a peer ID equals a UserId, that `peer=yes` proves a Player exists, or that game marker rules necessarily apply to Studio. |
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
| Properties/events | Game tables; pinned Studio property/event item writers and synthetic partial property headers (§11.7.1–2). | Team Create framing and tables, typed values/overrides, actual listener connections, directions, permissions and ordering. |
| Script editing | Studio operation JSON dispatches through a `SendOperation(int64, string)` reflection descriptor; a narrow type-0 transform was emulated (§11.7.3). | Native wire argument encoding/framing and direction, source delivery, atom semantics/units, timestamp/hash/ack meanings, complete multi-atom transforms and convergence behavior. |
| Collaborator presence | Pinned class fields, API/security and enum; selected local creation, service signal, status/idle and removal paths; six emulated setters (§11.8); cursor event descriptors/JSON builders (§11.7.4–5). | Native event framing/directions and typed codecs, caret units/timestamps, whether camera changes reach those setters, actual replication connections, initial presence and removal delivery. |
| Persistence | Local file readers/writers and a tested synthetic export. | Proof all necessary state is present and saved at a consistent collaborative revision; behavior for omitted/unavailable data. |

An authorized, minimal two-client trace with a precisely recorded action sequence and matching Studio build would be the most direct way to resolve many of these. Static analysis of additional pinned-build code can narrow hypotheses, but function names or isolated emulation are not a substitute for an end-to-end result. Keep the native adapter disabled/fail-closed when these facts are absent.

---

## 19. Sources and provenance

* Public Boblox game receiver, pinned revision: [repository](https://gitlab.com/kingdudely/Boblox/-/tree/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287), [schema and snapshot reader](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/solve/repl_snapshot.cpp), [value reader](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/solve/repl_core.cpp), [live item reader](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/solve/repl_stream.cpp), [runtime application](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/engine/live_runtime.cpp), [project findings](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/FINDINGS.md). Some older header/findings comments are superseded by later code/findings; this guide follows the pinned implementations where noted. No license was found in the inspected checkout; check its terms before copying source or captures.
* Join/player comparison in §2.4–2.5: public Boblox fixtures [`join97_new_frame.bin`](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/run/join97_new_frame.bin), [`repl_native97.bin`](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/run/repl_native97.bin), and [`msg02.bin`](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/tests/vectors/repl/msg02.bin), plus its [RbxTransport session](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/quic/session.cpp), [RakNet join builder](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/client/src/raknet/join_flow.cpp), and [game runtime instance code](https://gitlab.com/kingdudely/Boblox/-/blob/8c46b6dc175e22f50a7661bd19ff86eb5dbe4287/rbx_runtime/src/instance/instance.cpp). Fixture hashes and the offline synthetic parser check are recorded above.
* New outgoing-path evidence: [`roblox-team-create-rust/evidence/studio-outgoing/README.md`](roblox-team-create-rust/evidence/studio-outgoing/README.md), including Collaborator API excerpts and lifecycle decompilation, disassembly, RTTI, synthetic test results, saved harnesses, warning notes and SHA-256 manifest. No Studio capture is included.
* Pinned Studio static evidence in the previously supplied local research project: `roblox-team-create-rust/docs/FINDINGS.md`, `docs/CANDIDATE_SNAPSHOT.md`, `docs/VALIDATION.md`, `src/script_operation.rs`, `src/operation_transform.rs` and `evidence/`. The studied release was Windows Studio `0.745.0.7451137`, deployment `version-7062aec9dbe74b14`, EXE SHA-256 `781b36ed6fb62e61292b9e6a64327280c7da12b00f3ea8b2c91a3f4834f860ec`. Static code addresses are specific to that pinned image; no Studio session was executed.
* [Official Studio archive for the pinned deployment](https://setup.rbxcdn.com/version-7062aec9dbe74b14-RobloxStudio.zip) and [matching API dump](https://setup.rbxcdn.com/version-7062aec9dbe74b14-API-Dump.json). The archive and executable were downloaded from the CDN and hash-checked for the earlier static research.
* [Roblox Live Scripting announcement](https://devforum.roblox.com/t/live-scripting-beta/2640607), [collaboration documentation](https://github.com/Roblox/creator-docs/blob/main/content/en-us/projects/collaboration.md), [collaborator location announcement](https://devforum.roblox.com/t/jump-to-your-collaborator%E2%80%99s-location-in-team-create/2618317), [collaborator visibility announcement](https://devforum.roblox.com/t/improved-visibility-of-collaborators-in-studio/2893919), [ScriptDocument API](https://create.roblox.com/docs/reference/engine/classes/ScriptDocument), [ScriptEditorService API](https://create.roblox.com/docs/reference/engine/classes/ScriptEditorService), and [DraftsService API](https://create.roblox.com/docs/reference/engine/classes/DraftsService). These are behavior/API references, not native packet specifications.
* [`rbx_binary` 3.0.1 reader](https://docs.rs/rbx_binary/3.0.1/rbx_binary/fn.from_reader.html), [`to_writer`](https://docs.rs/rbx_binary/3.0.1/rbx_binary/fn.to_writer.html), and [`rbx-dom` repository](https://github.com/rojo-rbx/rbx-dom) for local file and DOM operations.
