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
- `avl.rs`, `list.rs`, `map.rs`: Persistent data structures for layout processing
- `order.rs`, `util.rs`: Supporting utilities

### Upstream references

Both the compiler and its data-structure layer are ports, and the originals are
the ground truth when behaviour diverges. If the OCaml packages are installed
(see TESTING.md) the sources sit at:

- `~/.opam/default/lib/typeset/Typeset.ml` — the compiler passes and renderer
- `~/.opam/default/.opam-switch/sources/cps_toolbox.0.3/lib/{Avl,Map,List}.ml` —
  the originals for `avl.rs`, `map.rs`, `list.rs`

Ordering matters in the data-structure layer: `structurize` feeds `Map::values`
straight into its graph construction, so the in-order guarantee of
`avl::to_list` is load-bearing for grp/seq nesting, not just a convenience.

The AVL is used only as an ordered map keyed by small integers, and only its
in-order/lookup behaviour is relied on. It does not guarantee strict AVL balance
for every insertion/removal order (a known limitation of this functional-AVL
port; see `avl::check_structural`). That is performance-only — contents and
ordering are always correct — so rendering is unaffected.

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
`max_depth` bound in `compile_safe_with_depth` is now a resource limit rather
than a stack-safety guard (see `TWO_BUFFER_DESIGN.md`).

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