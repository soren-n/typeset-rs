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