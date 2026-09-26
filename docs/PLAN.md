# Whiteboard: high-level plan

Status: locked 2026-09-26. Implementation detail is decided per milestone.

## Problem

Designing implementations with an LLM through words alone is low fidelity. Architecture and design
conversations happen faster and land better when both sides can point at, draw on, and rearrange a
shared picture. Whiteboard is a shared canvas where a human and an agent brainstorm and refine
designs together, and it should feel like working live with a teammate.

## Decisions

| Decision | Choice |
|---|---|
| Where the thinking happens | The agent session (Claude Code, Codex, pi, …) is the brain. Whiteboard is a tool it drives. No LLM calls inside the app. |
| Agent interface | An AXI-style CLI, `wb`, first. An MCP adapter can come later over the same core. |
| Source of truth | Our own semantic model and event log. The canvas library is a swappable renderer and editor behind an adapter. |
| Canvas | Excalidraw to start (MIT, hand-drawn look). Building our own canvas later stays possible because of the adapter boundary. |
| Agent edits | Land directly on the board, live, one at a time. No propose/accept step. |
| Storage | SQLite inside the app, so `wb` stays one self-contained binary. Postgres was considered; it only earns its place if boards become shared or hosted. Portable exports for sharing and archiving. |
| How it's used | A `whiteboard` skill is the entry point for people. `wb` is the engine the agent drives. This repo ships both as a Claude Code plugin. |
| Audio | Out of scope. Users bring their own voice-to-text. |
| Backend language | Rust |

## How it's used

Three layers, and a person only touches the top one:

1. **The skill**: `whiteboard <Linear or Jira URL | ticket ID | freeform idea>`. This is what you say
   to the agent.
2. **The canvas**: a browser tab (a bb browser tab works) where you draw, annotate, and hand over the
   turn.
3. **`wb`**: the command line the agent calls behind the scenes. A person runs it only for
   housekeeping (`wb boards`, `wb open`, `wb export`).

A session from a ticket:

1. You say `whiteboard LIN-123` in a thread whose working directory is the repo being designed, so
   the agent can read the code.
2. The agent reads the ticket through whatever tracker MCP is connected and reads the relevant code.
3. It creates a board linked to the ticket, then sketches its first take live: how things work now,
   the change the ticket implies, and open questions as notes.
4. It opens the canvas and asks what's wrong with its picture.
5. You circle, draw, and type, then hand over the turn. The agent reacts on the board and in its
   reply. Repeat.
6. When you say you're done, the agent leaves:
   - a summary of decisions and open questions
   - the exported board
   - a paste-ready ticket comment draft (never posted without approval)

Running `whiteboard LIN-123` again resumes the same board, because boards record their source.

The board then feeds later work. For example, greybeard's `implement` can take a board as its
requirements.

The skill runs wherever the agent is working, so `wb` must be on the PATH (`cargo install`), and the
plugin is installed at user scope.

This repo ships a Claude Code plugin containing:

- the skill
- a hook that shows the active board's state when a session starts (AXI's "ambient context")

If it later needs a panel inside bb itself, that bb plugin lives in `~/workspace/bb-plugins`.

## Architecture

One Rust binary, `wb`, that ships with the web canvas embedded in it.

```
agent session ──(shell)──> wb <verb> ──┐
                                        ├──> SQLite (WAL)
browser canvas <──SSE / POST──> wb serve ┘
```

- `wb` and `wb serve` share the SQLite file. SQLite transactions order events, and `wb serve` watches
  the file and streams every change to connected canvases. `wb open` starts the server when it isn't
  running. (Changed in M1 from "the server is the only writer": inside the Claude Code sandbox a
  sandboxed `wb` can't reach a server started outside it, but a server it starts is reachable from the
  browser. Sharing the file needs one sandbox allowance, write access to the data folder, instead of
  two, and `wb` works with no server running.)
- `wb <verb>` is the agent's interface. Each command does one thing, and its output tells the agent
  what happened and what it might do next.
- The canvas is a Vite + TypeScript + React app wrapping Excalidraw, embedded in the binary and opened
  at a localhost URL (a bb browser tab works fine).
- The Rust core (model, events, replay, geometry) is a library with no I/O, so the CLI, server, and a
  future MCP adapter all sit on the same logic.

## Data model

### Semantic model

The board is described by meaning, not pixels:

- **Node**: id, label, kind (service, db, queue, actor, note, …), and a layout hint
- **Edge**: from, to, label
- **Group**: a named set of nodes
- **Annotation**: a freehand stroke, the items it resolved to (enclosed, connected, struck through),
  and the text the author attached to it

Every item records whether a human or the agent made it. Ids are short and readable (`n4`, `e2`,
`a7`) so the agent can refer to them cheaply.

### Event log

The board is an append-only log of events. The board at any point is the log replayed up to that
event. That one structure gives three things:

- the final board, by replaying everything
- replay, by stepping through events
- "what changed since the agent last looked", by replaying a range

Each event has:

- sequence number
- timestamp
- actor (human or agent)
- turn number
- kind: `node.added`, `node.moved`, `node.relabeled`, `edge.added`, `annotation.added`, `item.deleted`, …
- payload

Turn markers carry the human's message and the agent's reply, and agent events may carry a short
"why". Replay shows the reasoning, not only shapes appearing.

Canvas gestures are coalesced: a drag commits one `node.moved` when the pointer lifts, so the
timeline reads as distinct actions.

The format is versioned from the first event (`wb/0.1`), and a JSON Schema is generated from the
Rust types.

### Storage

SQLite in the app's data directory:

- **`boards`**: id, title, where it came from (a ticket or an idea), timestamps
- **`events`**: the log. This is the source of truth.
- **`turns`**: turn boundaries plus the human's message and the agent's reply
- **`snapshots`**: a replayed board cached every N events. It's derived data, so it can be dropped
  and rebuilt.

### Portability

- `wb export <board>` writes a self-contained JSON Lines file: a header line (format version and
  board metadata), then every event. `wb import` reads it back.
- Snapshot exports to [JSON Canvas](https://jsoncanvas.org) and Mermaid, for Obsidian, tickets, and
  PRs. These lose annotations and history. They're outputs, not the format.

## The agent's interface

The agent's interface follows [AXI](https://axi.md/): compact text output, precomputed summaries, a
live view when run with no arguments, a next-step hint after each command, and structured errors.
The verbs sketched below are directional, not final:

- `wb`: the current board and turn state
- `wb look`: the board as compact text, with annotations already resolved (e.g. `a3: human circled
  n2 "Order Service" and n5: "this needs to be async"`). `--png` for when the layout itself matters.
- `wb wait`: blocks until the human hands the turn over, then returns their message and what changed
  since the agent last looked. This is how the agent is woken; channels aren't needed.
- `wb add` / `connect` / `group` / `note` / `move` / `delete`: intent-level edits. Code does the
  layout, so the agent never places pixels.
- `wb say "…"`: the agent's reply, shown on the canvas.
- `wb status "…"`: presence ("thinking about the queue boundary").

If the human changes something while the agent is working, the agent's next command output says
so, and the agent is never silently overwritten.

## Translating the canvas for the agent

Freehand strokes are classified in Rust, on geometry:

- a closed loop selects what it encloses
- a line between two items proposes a connection
- a scribble over an item strikes it

A stroke opens a small text box anchored to it. The stroke plus its text is the core human gesture.
Screenshots are available on demand but are never the main channel.

## Feeling like a live teammate

- Agent edits reach the canvas one at a time as the agent works, with a short animation, never as one
  batch at the end of a turn.
- The agent has a visible presence: a cursor near what it's working on, a status line, and its reply
  in a speech bubble or side panel.
- Agent marks look different from human marks.
- The human hands over the turn explicitly (a button or Cmd+Enter). Handing over after a period of
  idleness is a later option.
- The human can keep drawing while the agent works.

## v2: drawing from an iPad or phone

The goal: run the skill, get a URL (plus a QR code in the terminal), open it on an iPad or phone, and
draw back with Apple Pencil or a finger.

How to reach it:

- **BB Connect**: `bb connect expose <port>` gives an https URL on its own subdomain, gated by the
  owner's getbb.app session.
- **Without bb**: Tailscale or the local network, which needs our own auth.

v2 itself is deferred. These choices go into v1 so it stays cheap:

- **Nothing assumes localhost.** The web app uses relative paths and builds its WebSocket URL from the
  page's own address (`wss` behind https), so any host or proxy works.
- **The sync protocol doesn't depend on WebSockets.** Keep the messages independent of the transport,
  so a fallback (server-sent events for updates, HTTP POST for edits) can be added if a proxy won't
  carry WebSockets. Whether BB Connect carries them is unverified.
- **Reconnection is built in.** Mobile connections drop when the screen sleeps or the tab goes to the
  background.
  - Clients reconnect with "send me everything after event N", and the event log makes that a
    replay.
  - Client edits carry a client-generated id, so a retried send isn't applied twice.
- **One human, many devices.** A laptop and an iPad on the same board at once is normal, not
  multi-user. The server broadcasts to every connected client, and events record which device they
  came from.
- **Two APIs, split from the start.**
  - The agent's API (what `wb` calls) only listens on localhost.
  - The canvas API is the only thing that ever gets exposed, and it's scoped to one board.
  - Canvas URLs carry a per-board token even on localhost, so going remote changes the host, not
    the design.
- **Board coordinates only.** Never store screen coordinates. Stroke points are `[x, y, t, pressure?]`
  in board units, so Pencil pressure and per-point timing fit without a format change.
- **Stroke classification tolerates sloppy input.** Finger circles don't close neatly.
  - Tolerances scale with stroke size and the zoom level at the time, never fixed pixels.
  - Record the zoom level with each stroke.
- **Server-side rendering.** `wb look --png` shouldn't depend on a particular browser being open.
  Render from the semantic model in Rust (e.g. with resvg), including cropping around one annotation.
  That crop is also the fallback for reading handwriting when there's no typed text.
- **Real text inputs for comments.** The "circle, then type" box should be a real HTML text field.
  That gives iPad handwriting-to-text (Scribble) and keyboard dictation for free, which covers
  "bring your own voice-to-text" on mobile.
- **Touch-first controls.** The turn handoff must be touch-friendly, and the side panel collapses on
  small screens. Excalidraw already handles pen, touch and pressure.

## Later: native apps

The web canvas comes first. A native app is an option later if it earns its place.

What a native app would buy:

- **Tauri desktop app**:
  - wraps the same web canvas and Rust core in one app, so no browser tab
  - native notifications ("your turn")
  - a global hotkey to hand over the turn
  - a menu bar presence
- **Tauri on iOS / Android**: ships the same canvas and core to the iPad as an app.
- **A native iPad app (Swift + PencilKit)**:
  - the best Pencil feel: lowest latency with predicted strokes, hover, and squeeze gestures
  - offline drawing
  - push notifications without the home-screen web-app route

  Worth building only if Pencil feel turns out to be the bottleneck.

v1 keeps these open by:

- keeping the Rust core free of I/O, so Tauri can embed it (and Swift could call it through UniFFI)
- keeping the web canvas a standalone static bundle that only talks to the canvas API
- treating the canvas API and the JSON Schema as the contract any client can implement
- leaving room in stroke points for optional Pencil data (tilt, azimuth) beyond pressure

## Milestones

| Milestone | Delivers | Proves |
|---|---|---|
| M0: core + store + CLI | Semantic model, events, replay, JSON Schema, SQLite store, `wb` editing and looking at a board with no UI, export/import | The format holds up, and an agent can build a board by command |
| M1: live canvas | `wb serve`, embedded Excalidraw adapter with two-way sync and coalescing, turn handoff, `wb wait`, agent presence | Round trips with the agent feel live |
| M2: annotations | Stroke plus text, enclosure / connection / strike resolution, `wb look` tuned for the agent | The riskiest assumption: the agent understands human marks |
| M3: replay + export | Replay viewer with a scrubber, JSON Canvas and Mermaid export | The "watch how it was built" artifact |
| M4: integration | The Claude Code plugin: a `whiteboard <ticket\|idea>` skill (fetches the ticket, seeds the board, opens the tab, runs the wait loop, leaves a summary and comment draft) and the session-start hook | Everyday use, starting from a Linear ticket |

Early in M1, check that the agent's `wb` calls can reach the local server from inside the Claude Code
sandbox.

## Testing

Test first, testing behavior. Most of the system can be tested without a browser:

- replaying the log (including property tests: replaying to any point equals the snapshot there)
- stroke geometry and classification
- exact CLI output, via snapshot tests
- the SQLite store, round-tripped through export and import

A fixture set of boards and annotations with the interpretation we expect makes "does the agent
understand my marks" measurable. Browser end-to-end tests come in M1.

## Future ideas

### Altitude slider

A slider that sets how much detail the board shows. At the top, a handful of boxes for the whole
system. Moving it down reveals the pieces inside each box, down to classes and methods (roughly the
C4 levels: context, containers, components, code).

- Each node gets an optional altitude. A node can also sit inside a parent node, which it expands
  into at lower altitudes, so a group can collapse into a single box.
- The agent sketches high first, then adds detail below a box when asked ("zoom into CreatePayment").
  `wb look --altitude N` returns only what's visible at that level, which also keeps the agent's
  context small on big boards.
- Marks and notes belong to the altitude they were made at.
- This is additive to the `wb/0.1` format (optional fields), so it doesn't need a new version.

### Faster turns

Round trips should feel conversational. Ideas to try:

- Think hard on the opening sketch (reading the ticket and code), then drop to medium or low effort
  for follow-up turns, which are usually small corrections. The skill could hand follow-ups to a
  faster subagent, or tell the agent which effort to use per turn.
- Measure before tuning. The log already has timestamps, so a `wb stats` could report, per turn,
  time from the human's handover to the agent's first edit and to its reply.
- Keep edits streaming so the board moves within seconds even when the full answer takes longer.

## Deferred

- Remote access from an iPad or phone (v2; see its section for what v1 does to prepare)
- Native apps (see "Later: native apps")
- MCP adapter
- A propose/accept mode for agent edits
- A custom canvas replacing Excalidraw
- More than one human on a board
- Handing over the turn after idleness
- Audio
