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
  - `constructors/`: Layout building primitives (text, composition, control, format, etc.)
  - `passes/`: Compilation passes (denull, reassociate, linearize, serialize, etc.)
  - `render/`: Text rendering engine
  - `types/`: Core data structures (Layout, Doc, intermediate representations)
The compiler passes use standard-library collections throughout — the shared
custom data-structure layer is gone. Sequences and LIFO working stacks are
`Vec<T>` (or, when they must outlive a pass in the bump arena, arena slices
`&'a [T]` built with `alloc_slice_copy`); integer-keyed maps are `HashMap` (the
renderer's pack marks — point lookup/insert only, no ordering needed) or
`BTreeMap` (`structurize`'s open-scope property map — see below). The former
custom `avl.rs`/`map.rs`/`order.rs`/`list.rs` layer (a faithful port of the OCaml
`cps_toolbox` AVL/Map/List), and the `util.rs` closure-composition helper it
used, have been removed.

The one deliberate exception is `serialize`, which keeps two small local
cons-list structs (`TermList`/`CompList`) for its nest/pack and grp/seq path
accumulators. Unlike the removed `List`, these are genuinely persistent: at a
`Comp`/`Line` node both operands capture the same parent accumulator, and comp
accumulators are also captured into the emitted entries, so the tails are shared
across branches. A `Vec` would force a clone at every branch, so the shared
cons-list is the right structure here and stays.

### Upstream references

The compiler is a port, and the OCaml original is the ground truth when
behaviour diverges. If the OCaml packages are installed (see TESTING.md) the
source sits at:

- `~/.opam/default/lib/typeset/Typeset.ml` — the compiler passes and renderer

Ordering matters in `structurize`: it feeds the property map's values straight
into its graph construction, so the values must come out in key order. This is
why that map is a `BTreeMap` (ascending-key iteration) and not a `HashMap` — the
in-order guarantee is load-bearing for grp/seq nesting, not just a convenience.
The map is keyed by small integers and threaded linearly (each update replaces
the binding; no earlier version is retained), so an owned map mutated in place
is a faithful replacement for the former persistent map.

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

**Stack usage:** the entire pipeline runs iteratively — the ten transform passes
in `passes/`, pass 10 (`move_to_heap`, moving the document from the bump
allocator to the heap `Doc`), the renderer, and the `Drop` of `Doc` are each a
descend/ascend trampoline over a heap-allocated frame stack (continuation-passing
passes had their continuation chains defunctionalized into explicit data). Every
stage therefore uses constant native stack regardless of layout depth, so deep
layouts never overflow the stack; depth shows up as O(depth) heap instead. The
tree-walking traits on the public AST types (`Doc`/`DocObj`/`DocObjFix` and
`Layout`) — `Drop`, `Clone`, `Display`, and `Debug` — are iterative for the same
reason, so no operation on a deep document recurses on the native stack. The
`max_depth` bound in `compile_safe_with_depth` is now a resource limit rather
than a stack-safety guard.

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