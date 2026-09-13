# Architecture

## Project structure

A Rust workspace of three crates:
- **typeset**: the layout language, compiler, renderer, and the DSL parser
  (`typeset::dsl`)
- **typeset-parser**: the `layout!` procedural macro, a token adapter over
  `typeset::dsl`
- **oracle/driver** (`typeset-oracle-driver`, unpublished): the binary the
  OCaml oracle harness renders through

## typeset crate (`typeset/src/`)

- `lib.rs`: the crate doc (the crate README), the public API (`Layout`,
  `Doc`, `Pad`, `Break`, the constructors), the pass table and
  `Layout::compile`
- `constructors.rs`: the functions users build layouts with
- `layout.rs`: `Layout`, the public input type
- `dsl.rs`: the DSL grammar, parser and run-time front end
- `arena.rs`: the arena primitives every representation is built from
- `serialize.rs`, `resolve_scopes.rs`, `lower.rs`: the three passes, each
  owning the representation it produces
- `doc.rs`: `Doc`, the public output type, and its builder
- `render.rs`: `Doc::render`

### Arenas, ids, ranges

Every representation, the public `Layout` and `Doc` included, is a flat
postorder arena: nodes live in a `Vec`, children precede their parents, and
a node refers to its children by index. `arena.rs` gives that shape types:

- `Arena<T>` is an append-only `Vec<T>` whose `push` returns an `Id<T>`.
- `Id<T>` is a `u32` index usable only with arenas and side tables of
  element type `T`, so a cross-arena index mix-up is a type error. It is
  non-zero internally, so `Option<Id<T>>` is the same four bytes and is how
  every absent link is expressed; there are no sentinel values.
- `IdVec<K, V>` is a side table with one `V` per element of an `Arena<K>`.
- `Range<T>` is a `[start, end)` pair of `u32` offsets into a shared buffer
  (`Range<str>` for text): how a representation refers to a sub-sequence
  without owning a `Vec`.

Because everything is flat, every pass is a loop: a bottom-up fold runs
forward over the arena, inherited context runs backward. No stage recurses
on the native stack, so a layout of any depth compiles and renders; depth
costs heap.

### `Layout`

`Layout` is opaque: a postorder `Arena<LayoutNode>` with the root last and
one text buffer that text nodes range into. The empty layout is the empty
text. Unary constructors push a node; binary constructors append the
smaller operand's arena onto the larger (shifting its ids and text ranges)
and push the parent. Building `n` nodes is O(n) for left- or right-leaning
chains and O(n log n) in the worst (balanced) case. `Clone`, `Drop` and
`Debug` derive; cloning is two allocations.

### Pipeline

| Pass             | Lowers                    | Does |
|------------------|---------------------------|------|
| `serialize`      | `Layout` → `FixedDoc`     | split into lines at hard breaks and inside broken sequences; coalesce runs of fixed compositions; record scope deltas |
| `resolve_scopes` | `FixedDoc` → `RebuildDoc` | build, solve and read back the grp/seq scope graph per line |
| `lower`          | `RebuildDoc` → `Doc`      | drop empty terms; eliminate trivial grp/seq; right-associate; factor shared nest/pack prefixes; build the `Doc` and its extent tables |

**serialize.** One left-to-right DFS with an explicit stack. It threads:
- the innermost nest/pack wrapper, as an id into a shared path arena (one
  node per wrapper descended through, so sibling leaves share their spine);
- the innermost grp/seq wrapper, as an id into a parent-linked chain arena.
  Scopes nest, so the scopes open at any point of a line form a stack: a
  composition records how many scopes close at it and which open (outermost
  first), by diffing its chain against the previous composition's along
  their shared spine, an O(delta) walk. Carrying deltas rather than full
  scope stacks is what keeps deeply nested scopes linear;
- `fixed` (under a `fix`, every surviving composition is fixed) and `broken`
  (under a `seq` whose subtree contains a hard line: the seq is dropped and
  its breakable compositions become lines; `fix` and `grp` reset it). The
  line decision uses the composition's own attribute, before the fix
  override.

Every item of a line is a run: one or more terms joined by fixed
compositions, which never breaks. Lines, runs, terms and separators are
ranges into shared buffers, so nothing is allocated per line or per run.

**resolve_scopes.** Scopes are ranges over a line's items, and items only
exist once fixed compositions have coalesced into runs, so a scope's extent
cannot be read off the tree. Per line, every item is a graph node and every
scope an edge from the node it opened at to the node it closed at, built by
replaying the stack deltas in opening order, which is document pre-order.
A node has both incoming and outgoing edges only when a run straddles a
scope boundary (`grp(a + b) !& c`: the run `[b c]` both closes the grp and
follows it). `solve` resolves those by widening: leading seq out-edges are
re-sourced onto the incoming side, and the incoming list is handed forward
past the first grp out-edge, with tie-breaks that depend on edge-list
order. `rebuild` reads each line back as a composition spine with grp/seq
wrappers, using a stack of open scopes over one partial spine.

The graph is a side table over the item buffer (a node is its item's id)
plus one edge arena; a node's incident edges are intrusive linked lists
through that arena, so every list move is O(1). This is the reference
implementation's formulation and its widening rules are defined over it; a
tree rewrite would re-encode the same item ranges less directly.

**lower.** The reference runs five tree rewrites here (null removal, seq and grp
identity elimination, reassociation, rescoping); `lower` applies the same
rules, in the same non-confluent order, as three loops with side tables
over the rebuilt arena:
1. forward: survival (empty texts vanish, wrappers and all), the pad a
   composition with a vanished left operand forwards to its left, the seq
   count (a grp is opaque to it), and the lowered runs (empty terms dropped,
   pads between survivors merged, the first survivor's wrappers kept);
2. backward: whether each node is directly under a seq and whether it is at
   the head of its group, which decides which seqs survive (fewer than two
   compositions, or directly under a seq: dropped);
3. forward: the grp count and grp survival (no compositions, or at the head
   of its group: dropped), then each composition tree threaded as a chain
   of atoms and materialized right-nested at every surviving wrapper and
   line root, factoring at each composition the nest/pack prefix its
   operands share, straight into the `Doc` builder.

### `Doc` and the renderer

`Doc` is one optional root object per line (`None` for an empty line), an
object arena, a run buffer (each entry a text with the pad before it), one
text buffer, and two side tables: each object's mid-line extent and its
mid-line distance to the first composition boundary. Mid-line, neither nest
nor pack advances the position, so both are exact state-independent sums
computed once in `DocBuilder::finish`.

The renderer walks the arena with an explicit frame stack. `should_break`
is arithmetic on the boundary table. `will_fit` is arithmetic on the extent
table mid-line; at the head of a line indentation offsets depend on the
live level and pack marks, but offsets are only emitted before the first
text on the line, so the head-of-line measure walks the object's left
spine and adds the flat extent. Pack marks are a dense `Vec<Option<usize>>`
keyed by pack index. Lines are joined by newlines.

### Semantics worth knowing

These follow from the reference and are pinned by tests:
- `null` is `text("")`; both vanish together with any wrappers on them.
- A fixed composition's run keeps the nest/pack wrappers of its *first*
  literal, so `fix_unpad(text("("), pack(args))` takes the first argument
  out of the pack. Fix a delimiter to what precedes it; compose an opening
  delimiter with `unpad`.
- A `seq` that does not fit breaks every composition beneath it, including
  inside nested seqs; only a `grp` resets that.
- Width is counted in `char`s, not bytes (the reference counts bytes; this
  is the one deliberate divergence) and not display columns.

## Upstream reference

The compiler is a port; the OCaml original is the ground truth when
behaviour diverges, and every change is held to byte-identical output
against it (see DEVELOPMENT.md). With the OCaml packages installed the
source sits at `~/.opam/default/lib/typeset/Typeset.ml`.

## typeset-parser crate (`typeset-parser/src/`)

`lib.rs`: the `layout!` macro. It flattens its Rust token trees into
`typeset::dsl::Token`s (spans as positions), hands them to
`typeset::dsl::parse_tokens`, and builds constructor calls through the
`Build` trait, so the macro and `typeset::dsl::parse` share one grammar
implementation. A bare identifier is a variable: a `Layout` in scope,
cloned.

## Layout language

- **Text** `text()`, **empty** `null()`
- **Composition** `comp(l, r, Pad, Break)`: `Pad` chooses a space between
  operands, `Break::Fixed` forbids the composition from breaking. Shortcuts
  `pad`/`unpad`/`fix_pad`/`fix_unpad`; DSL `+ & !+ !&`
- **Hard break** `line(l, r)`; DSL `@`, and `@@` for a blank line
- **Wrappers**: `fix` (never breaks inside), `grp` (compositions inside
  break all-or-nothing), `seq` (once one composition breaks, all later ones
  do), `nest` (continuation lines indent by one tab), `pack` (continuation
  lines align to the column of the first element)
- **Joins**: `join_with_spaces`, `join_with_commas`, `join_with_lines`
