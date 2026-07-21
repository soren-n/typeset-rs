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
  - `passes/`: Compilation passes (denull, reassociate, linearize, serialize, etc.)
  - `render/`: Text rendering engine
  - `types/`: Core data structures (Layout, Doc, and the intermediate
    representations shared between passes; an IR used by only one pass lives with
    that pass instead — e.g. `Broken` in `passes/broken.rs` and the structurize
    scope graph in `passes/structurize/graph.rs`)
The compiler passes use standard-library collections throughout — the shared
custom data-structure layer is gone. Sequences and LIFO working stacks are
`Vec<T>` (or, when they must outlive a pass in the bump arena, arena slices
`&'a [T]` built with `alloc_slice_copy`); integer-keyed maps are `HashMap` (the
renderer's pack marks — point lookup/insert only, no ordering needed) or
`BTreeMap` (`structurize`'s open-scope map, keyed by scope index — see below).
The former
custom `avl.rs`/`map.rs`/`order.rs`/`list.rs` layer (a faithful port of the OCaml
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

Ordering matters in `structurize`: each grp/seq scope becomes one graph edge,
and the order edges are created fixes every node's incoming/outgoing edge lists,
which `solve` and `rebuild` then consume. Scopes arrive as per-composition
open/close deltas (computed in `serialize`); `graphify` replays them per line —
an open records a scope's `from` node, a close pairs it with a `to` node — then
sorts the resulting edges by scope index before materializing them, so the graph
is always built in a deterministic, ascending-index sequence. That sort is the
load-bearing ordering guarantee for grp/seq nesting, not just a convenience. The
still-open scopes are held in a `BTreeMap` keyed by their small integer index;
the map is threaded linearly (an open inserts, a close removes) and its
iteration order does not matter, since the edges are sorted explicitly.

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

**Stack usage:** the entire pipeline runs iteratively — the nine transform
passes in `passes/` (the last, `rescope`, builds the owned heap `Doc` directly
from the bump-allocated `DenullDoc`), the renderer, and the `Drop` of `Doc` are
each a descend/ascend trampoline over a heap-allocated frame stack
(continuation-passing passes had their continuation chains defunctionalized into
explicit data). Every
stage therefore uses constant native stack regardless of layout depth, so deep
layouts never overflow the stack; depth shows up as O(depth) heap instead. The
tree-walking traits on the public AST types (`Doc`/`DocObj`/`DocObjFix` and
`Layout`) — `Drop`, `Clone`, `Display`, and `Debug` — are iterative for the same
reason, so no operation on a deep document recurses on the native stack.
`compile()` is therefore infallible and imposes no depth cap; the `max_depth`
bound in `compile_within_depth` is an opt-in resource limit (capping the
O(depth) heap an untrusted layout can allocate) rather than a stack-safety
guard.

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