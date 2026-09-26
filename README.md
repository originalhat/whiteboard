# Whiteboard

A shared canvas where you and a coding agent sketch designs together. The agent drives `wb` from
its shell; you draw in the browser.

## Install

```sh
(cd web && npm install && npm run build)
cargo install --path crates/wb
```

## Agent commands

```sh
wb new "Checkout redesign" --source LIN-123   # resumes the board if the ticket already has one
wb add service "Order Service"                # kinds: service db queue actor external component note
wb add db "Orders DB" --right-of n1           # or --left-of --below --above --near
wb add component "CreatePayment" --in n1      # detail inside n1, one altitude lower
wb connect n1 n2 "writes"
wb note "Sync or async?" --on e1
wb say "Here's my first take"
wb open                                       # starts the server, prints the canvas link
wb wait                                       # hands you the turn, returns your reply
wb look [--altitude 1]                        # the board as text, optionally one level only
```

`wb` with no arguments shows the current board. `wb --help` lists the rest (`move`, `relabel`,
`group`, `level`, `delete`, `status`, `log`, `export`, `import`).

## On the canvas

- Drag, draw labelled shapes, draw arrows between boxes, and edit labels as usual.
- Circle, connect or scribble with the pen to mark things; add a comment in the box that opens.
- Select something before handing over to say what "this" is.
- Hand over with ⌘↵. What the agent changed last turn is shaded.
- The altitude slider (on boards that use levels) goes from the whole system down to code.

## Notes

- Boards live in `~/Library/Application Support/whiteboard` (set `WB_HOME` to move them).
- In the Claude Code sandbox, allow writes to that folder, or `wb` can't open its database.
- See [docs/PLAN.md](docs/PLAN.md) for the design and roadmap.
