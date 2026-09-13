# Architecture

## Project Structure

A Rust workspace of three crates:
- **typeset**: the layout language, compiler, renderer, and a runtime DSL
  parser (`typeset::dsl`)
- **typeset-parser**: the `layout!` procedural macro (compile-time DSL)
- **tests/differential** (`typeset-differential`, unpublished): the driver
  the OCaml differential harness renders through

## typeset crate (`typeset/src/`)

- `lib.rs`: public API (`Layout`, `Doc`, `Pad`, `Break`, the constructors,
  `compile`, `render`, `format_layout`) and the `dsl` module
- `dsl.rs`: runtime parser for the layout DSL, iterative (parenthesis depth
  costs heap, not stack)
- `compiler/constructors.rs`: the constructor functions users build layouts
  with
- `compiler/pipeline.rs`: `compile`, the authoritative pass table
- `compiler/passes/`: one module per pass; each owns the representation it
  produces
- `compiler/render.rs`: `render` and `Doc::render`
- `compiler/types/`: `arena.rs` (the arena primitives), `layout.rs` (the
  public input type), `ir.rs` (the vocabulary passes share), `doc.rs` (the
  public output type)

### Arenas, ids, ranges

Every representation in the pipeline, the public `Layout` and `Doc` included,
is a flat postorder arena: nodes live in a `Vec`, children precede their
parents, and a node refers to its children by index. `types/arena.rs` gives
that shape types:

- `Arena<T>` is an append-only `Vec<T>` whose `push` returns an `Id<T>`.
- `Id<T>` is a `u32` index usable only with arenas and side tables of element
  type `T`, so a cross-arena index mix-up is a type error. It is non-zero
  internally, so `Option<Id<T>>` is the same four bytes and is how every
  "no parent" / "no next edge" / "not yet seen" link is expressed; there are
  no sentinel values.
- `IdVec<K, V>` is a side table with one `V` per element of an `Arena<K>`,
  indexed by `Id<K>`.
- `Range<T>` is a `[start, end)` pair of `u32` offsets into a shared buffer
  (`Range<str>` for text), which is how a representation refers to a
  sub-sequence without owning a `Vec` of its own.

Because everything is flat, every pass is a loop: a bottom-up fold runs
forward over the arena (children's results are already computed), inherited
context runs backward (parents first). No stage recurses on the native stack,
so a layout of any depth compiles and renders; depth costs heap.

### `Layout`

`Layout` is opaque: a postorder `Arena<LayoutNode>` with the root last and one
text buffer that text nodes range into. Unary constructors push a node; binary
constructors append the smaller operand's arena onto the larger (shifting its
ids and text ranges) and push the parent. Building `n` nodes is O(n) for
left- or right-leaning chains and O(n log n) in the worst (balanced) case.
`Clone`, `Drop` and `Debug` are derived; cloning is two allocations.

### Pipeline

| Pass             | Lowers                    | Does |
|------------------|---------------------------|------|
| `serialize`      | `Layout` → `FixedDoc`     | split into lines at hard breaks and inside broken sequences; coalesce runs of fixed compositions; record scope open/close deltas |
| `resolve_scopes` | `FixedDoc` → `RebuildDoc` | build, solve and read back the grp/seq scope graph per line |
| `denull`         | `RebuildDoc` → `DenullDoc`| drop null/empty terms; strip nest/pack paths to prop lists |
| `normalize`      | `DenullDoc` → `DenullDoc` | eliminate trivial grp/seq; right-associate compositions |
| `rescope`        | `DenullDoc` → `Doc`       | factor shared nest/pack prefixes; build the `Doc` and its extent tables |

Each pass's output type lives in its module; `types/ir.rs` holds what they
share: `Term` (a nest/pack path over a leaf), `PathNode`, `Prop`, `Scope`,
and the generic composition-tree nodes `Obj<T>` / `Fix<T>` that
`resolve_scopes` produces over `Term` and `denull`/`normalize` fold over
`DenullTerm`.

**serialize.** One left-to-right DFS with an explicit stack. It threads:
- the innermost nest/pack wrapper, as an id into a shared path arena (one
  node per wrapper descended through, so sibling leaves share their spine);
- the innermost grp/seq wrapper, as an id into a parent-linked scope-chain
  arena. A composition records the scopes that open and close at it by
  diffing its chain against the previous composition's on the same line;
  the chains share their outer spine by id and carry a depth, so the diff is
  an O(delta) walk to the common suffix. Carrying deltas (total size O(number
  of scopes)) rather than each composition's full scope stack is what keeps
  deeply nested scopes linear;
- `fixed` (under a `fix`: every surviving composition is fixed) and `broken`
  (under a `seq` whose subtree contains a hard line: the seq is dropped and
  its breakable compositions become lines; `fix` and `grp` reset it). The
  line decision uses the composition's own attribute, before the fix
  override.

The leaves are then laid out as lines of items, with maximal runs of terms
joined by fixed compositions coalesced into single fix items. Everything is
ranges into five shared buffers, so no per-line or per-run allocation.

**resolve_scopes.** Per line, every item is a graph node and every scope an
edge from the node it opened at to the node it closed at (`graphify`), built
in ascending scope-index order, which `solve` and `rebuild` depend on. A node
has both incoming and outgoing edges only when a fix run straddles a scope
boundary (`grp(a + b) !& c`: the item `[b c]` both closes the grp and follows
it). `solve` resolves those by widening: leading seq out-edges are re-sourced
onto the incoming side, and the incoming list is handed forward past the first
grp out-edge. `rebuild` then reads each line back as a composition spine with
grp/seq wrappers, using a flat continuation stack. Adjacency is intrusive
linked lists through one shared edge arena, so every list move is O(1).

**denull, normalize, rescope** are plain folds over `Obj`/`Fix` arenas. Nest
and pack props are ranges into one shared prop buffer, memoized per path id,
so `rescope`'s prefix factoring only ever produces subranges.

### `Doc` and the renderer

`Doc` is one optional root object per line (`None` for an empty line), an
object arena, a fixed-object arena, one text buffer, and two side tables:
each object's mid-line extent and its mid-line distance to the first
composition boundary. Mid-line, neither nest nor pack advances the position,
so both are exact state-independent sums computed once in `DocBuilder::finish`.

The renderer (`Renderer` over a `Config { width, tab }` and a
`Cursor { head, broken, lvl, pos }`) walks the arena with explicit frame
stacks. `should_break` is arithmetic on the boundary table; `will_fit` is
arithmetic on the extent table except at the head of a line, where
indentation depends on live state and it folds — a fold that stops as soon as
the position passes the width. Pack marks are a dense `Vec<Option<usize>>`
keyed by pack index. Lines are joined by newlines.

## Upstream reference

The compiler is a port; the OCaml original is the ground truth when behaviour
diverges, and every refactor is held to byte-identical output against it (see
TESTING.md). With the OCaml packages installed the source sits at
`~/.opam/default/lib/typeset/Typeset.ml`.

## typeset-parser crate (`typeset-parser/src/`)

`lib.rs`: the `layout!` macro, built on `syn`/`quote`/`proc-macro2`. It
expands each DSL node to the matching `typeset` constructor call, so the macro
is pure sugar over the constructor API. `typeset::dsl` accepts the same
language at run time.

## Layout language

- **Text** `text()`, **empty** `null()`
- **Composition** `comp(l, r, Pad, Break)`: `Pad` chooses a space between
  operands, `Break::Fixed` forbids the composition from breaking. Shortcuts
  `pad`/`unpad`/`fix_pad`/`fix_unpad`; DSL `+ & !+ !&`
- **Hard break** `line(l, r)`; DSL `@`, and `@@` for a blank line
- **Wrappers**: `fix` (never breaks inside), `grp` (compositions inside break
  all-or-nothing), `seq` (once one composition breaks, all later ones do),
  `nest` (continuation lines indent by one tab), `pack` (continuation lines
  align to the column of the first element)
