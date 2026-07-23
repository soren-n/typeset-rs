# Architecture

## Project Structure

This is a Rust workspace containing two main crates:
- **typeset**: A DSL for defining source code pretty printers
- **typeset-parser**: A procedural macro parser that provides compile-time DSL parsing for typeset

## Core Components

### typeset crate (`typeset/src/`)

**Primary modules**:
- `lib.rs`: Main public API exports (Layout, Doc, constructors, compile, render)
- `compiler/`: Core layout compiler and renderer implementation
  - `constructors.rs`: Layout building primitives (text, composition, control, joining, wrapping, one-step formatting)
  - `pipeline.rs`: The authoritative pass table — which pass lowers which
    representation, in what order
  - `passes/`: One module per pass (flatten, resolve_breaks, serialize,
    split_lines, resolve_scopes, denull, normalize, rescope)
  - `render/`: Text rendering engine
  - `types/`: Core data structures (Layout, Doc, and the intermediate
    representations shared between passes; an IR used by only one pass lives
    with that pass instead — e.g. the scope graph in
    `passes/resolve_scopes/graph.rs`)

The compiler passes use standard-library collections throughout — the shared
custom data-structure layer is gone. Sequences and LIFO working stacks are
`Vec<T>`; dense integer keys index plain `Vec`s (the renderer's pack marks,
keyed by the compiler's dense DFS pack counters); the one ordered map is
`BTreeMap` (`graphify`'s open-scope map, keyed by scope index — see below).
The former custom
`avl.rs`/`map.rs`/`order.rs`/`list.rs` layer (a faithful port of the OCaml
`cps_toolbox` AVL/Map/List), and the `util.rs` closure-composition helper it
used, have been removed.

The one deliberate exception is `serialize`, which keeps two small local
cons-list structs (`TermList`/`CompList`) for its nest/pack and grp/seq path
accumulators. Unlike the removed `List`, these are genuinely persistent: at a
`Comp`/`Line` node both operands capture the same parent accumulator, and comp
accumulators are also captured into the emitted entries, so the tails are shared
across branches. A `Vec` would force a clone at every branch, so the shared
cons-list is the right structure here and stays. That sharing is also load-
bearing for speed: `serialize` turns each composition's enclosing-scope list
into scope open/close *deltas* by diffing it against the previous composition's
list, and because the two lists share their outer tail by pointer the diff is a
short longest-common-suffix walk (`CompList` carries a `depth` field for it).
Carrying deltas — rather than each composition's full enclosing scope stack —
keeps the grp/seq passes linear on deeply nested scopes instead of O(n^2).

### Upstream references

The compiler is a port, and the OCaml original is the ground truth when
behaviour diverges. If the OCaml packages are installed (see TESTING.md) the
source sits at:

- `~/.opam/default/lib/typeset/Typeset.ml` — the compiler passes and renderer

Ordering matters in `resolve_scopes`: each grp/seq scope becomes one graph
edge, and the order edges are created fixes every node's incoming/outgoing edge
lists, which `solve` and `rebuild` then consume. Scopes arrive as
per-composition open/close deltas (computed in `serialize`); `graphify` replays
them per line — an open records a scope's `from` node, a close pairs it with a
`to` node — then sorts the resulting edges by scope index before materializing
them, so the graph is always built in a deterministic, ascending-index
sequence. That sort is the load-bearing ordering guarantee for grp/seq nesting,
not just a convenience. The still-open scopes are held in a `BTreeMap` keyed by
their small integer index; the map is threaded linearly (an open inserts, a
close removes) and its iteration order does not matter, since the edges are
sorted explicitly. The graph itself is index-linked: nodes and edges live in
`Vec`s per line, with in/out adjacency as ordered `Vec<EdgeId>` lists that
`solve` rearranges with plain insert/splice operations.

### typeset-parser crate (`typeset-parser/src/`)

- `lib.rs`: Procedural macro implementation for parsing layout DSL syntax
- Dependencies: `syn`, `quote`, `proc-macro2` for macro parsing

## Layout System Architecture

The library implements a two-phase pretty printing system:

### Phase 1: Layout Construction
Build layout trees using constructors:
- **Text**: `text()` - literal text nodes
- **Composition**: `comp()` - combine layouts with spacing/breaking behavior  
- **Control**: `nest()`, `pack()` - indentation management
- **Grouping**: `fix()`, `grp()`, `seq()` - break behavior control

### Phase 2: Compilation & Rendering
1. **Compilation**: `compile()` applies optimization passes to layout trees
2. **Rendering**: `render()` outputs formatted text with proper line breaks and indentation

**Stack usage and representation:** every intermediate representation is a
**flat structure** — postorder index arenas (children precede parents) or plain
vectors — so each pass is a loop over node indices: bottom-up folds run
forward (children's results already computed), inherited context runs backward
(parents visited first). `flatten` is the single step that walks the public
`Box`-recursive `Layout` tree; text is moved into the layout arena there and
borrowed through the rest of the pipeline. One bump arena remains, backing
`serialize`'s persistent scope accumulators (see above). Every stage uses
constant native stack regardless of layout depth, so deep layouts never
overflow; depth shows up as O(depth) heap instead. The output `Doc` is a flat
arena too — a `Vec<Row>` spine plus two index-linked `Vec`s of shallow nodes
(`ObjNode`/`FixNode`) — so `Clone`, `Drop`, and `Debug` are derived and
deep-safe *structurally*. `Layout` (the input AST) is the one `Box`-recursive
tree, so it keeps iterative `Drop`/`Clone`/`Debug` impls (see
`types/traversal.rs`). `compile()` is therefore infallible and imposes no depth
cap; layout depth shows up only as O(depth) heap, freed once compilation
returns.

**Renderer:** break decisions are O(1). Compilation precomputes each object's
flat mid-line extent and its mid-line distance to the first composition
boundary (mid-line, `head == false`, neither nest nor pack advances the
position, so both are exact state-independent sums stored in the `Doc`).
`should_break` compares arithmetic against the width; `will_fit` only falls
back to an actual measuring fold at the head of a line, where indentation
offsets depend on live state — and that fold is width-bounded (it stops the
moment the position passes the target width).

## Key Layout Concepts

### Composition Behavior
- **Padded vs Unpadded**: Whether spaces are inserted between elements
- **Fixed vs Breakable**: Whether line breaks are allowed at composition points
- **Operators**: `&` (unpadded), `+` (padded), `!&` (unpadded+fix), `!+` (padded+fix)

### Special Constructors
- `fix`: Treat content as literal (no breaks allowed)
- `grp`: Break as a group (all elements break together)
- `seq`: Sequential breaking (break all if any breaks)

### Indentation Types
- `nest`: Fixed-width indentation increase
- `pack`: Align to position of first literal character

### Line Breaking
- `@`: Soft line break (break if needed)
- `@@`: Hard line break (always break)