# Architecture

## Project structure

A Rust workspace of three crates:
- **typeset**: the layout language, compiler, renderer, and the DSL parser
  (`typeset::dsl`)
- **typeset-parser**: the `layout!` procedural macro, a token adapter over
  `typeset::dsl`
- **oracle/driver** (`typeset-oracle-driver`, unpublished): the process the
  OCaml oracle harness renders through

## typeset crate (`typeset/src/`)

- `lib.rs`: the crate doc (the crate README), the public API (`Layout`,
  `Doc`, `Pad`, `Break`, the constructors) and `Layout::compile`
- `constructors.rs`: the functions users build layouts with
- `layout.rs`: `Layout`, the public input type
- `dsl.rs`: the DSL grammar, parser and run-time front end
- `arena.rs`: the arena primitives every representation is built from
- `serialize.rs`, `structure.rs`: the two passes; `serialize` owns the
  line representation it lends, `structure` the graph it solves
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
- `Tree<T>` is a forest of parent-linked nodes with depths, so two chains
  that share their outer spine by id give their lowest common ancestor,
  and the nodes each has beyond it, in a walk of the difference. Both
  wrapper chains in `serialize` are trees.

Because everything is flat, every pass is a loop: a bottom-up fold runs
forward over the arena, inherited context runs backward, and a walk that
needs a stack keeps an explicit `Vec` of frames. No stage recurses on the
native stack, so a layout of any depth compiles and renders; depth costs
heap.

### `Layout`

`Layout` is opaque: a postorder `Arena<LayoutNode>` with the root last, one
text buffer that text nodes range into, and a flag saying whether a hard
line is anywhere beneath the root. The empty layout is the empty text.
Unary constructors push a node; `seq` pushes a `Broken` node instead of a
`Seq` when the flag is set, since such a sequence breaks unconditionally.
Binary constructors append the smaller operand's arena onto the larger
(shifting its ids and text ranges) and push the parent. Building `n` nodes
is O(n) for left- or right-leaning chains and O(n log n) in the worst
(balanced) case. `Clone`, `Drop` and `Debug` derive; cloning is two
allocations.

### Pipeline

| Pass        | Lowers                       | Does |
|-------------|------------------------------|------|
| `serialize` | `Layout` → lines of terms    | split into lines at hard breaks and inside broken sequences; mark each composition fixed or breakable; record scope deltas |
| `structure` | lines of terms → `Doc`       | per line: read the items off the glue, build and solve the grp/seq scope graph, then read it back into the `Doc`: drop empty terms, decide the grp/seq identities, right-nest every spine, factor shared nest/pack wrappers, measure every object |

Nothing crosses a hard line, so the passes are a pipeline of lines: the
`Serializer` runs its DFS up to the next hard line and lends that line, and
`Structure` consumes it with scratch reused across lines. The intermediate
is one line, whatever the document's size.

**serialize.** One left-to-right DFS with an explicit stack, paused between
lines. It threads:
- the innermost nest/pack wrapper, as a node of the path tree. The tree is
  a trie: a node has at most one `Nest` child and a `Pack` node is unique
  to its index, so two terms under the same wrappers hold the same node,
  and the wrappers two terms share are the chain of their lowest common
  ancestor. Sibling leaves share their spine, so path storage is O(input);
- the innermost grp/seq wrapper, as a node of the scope-chain tree.
  Scopes nest, so the scopes open at any point of a line form a stack: a
  composition records how many scopes close at it and which open (outermost
  first), by diffing its chain against the previous composition's along
  their shared spine, an O(delta) walk. Carrying deltas rather than full
  scope stacks is what keeps deeply nested scopes linear;
- `fixed` (under a `fix`, every surviving composition is fixed) and `broken`
  (under a `Broken` sequence, whose wrapper is dropped and whose breakable
  compositions become lines; `fix` and `grp` reset it). The line decision
  uses the composition's own attribute, before the fix override.

A line is its terms in order, each carrying the *glue* to the next: a
composition (pad, fixed or breakable, scope delta) or, on the last term,
the hard line. An item of a line is a run: one or more terms joined by
fixed compositions, which never breaks; `structure` reads the items off
the glue.

**structure: the graph.** Scopes are ranges over a line's items, and items
only exist once fixed compositions have been read as runs, so a scope's
extent cannot be read off the tree. Every item is a graph node and every
scope an edge from the node it opened at to the node it closed at, built
by replaying the stack deltas in opening order, which is document
pre-order. A node has both incoming and outgoing edges only when a run
straddles a scope boundary (`grp(a + b) !& c`: the run `[b c]` both closes
the grp and follows it). `solve` resolves those by widening: leading seq
out-edges are re-sourced onto the incoming side, and the incoming list is
handed forward past the first grp out-edge, with tie-breaks that depend on
edge-list order.

The graph's nodes are the line's items (each a range of its terms) plus one
edge arena; a node's incident edges are intrusive linked lists through
that arena, so every list move is O(1). This is the reference
implementation's formulation and its widening rules are defined over it; a
tree rewrite would re-encode the same item ranges less directly.

**structure: the emitter.** After `solve` a line's scopes nest, every node
either closes scopes or opens them, and the scopes open at any item form a
stack. So a line reads back as a tree of *spines*: the line's own and one
per scope, each a left-to-right sequence of elements (an item or a nested
scope) with a pad between neighbours. The line is walked twice with a
stack of open spines, applying the rules the reference runs as five
tree rewrites afterwards (null removal, seq identities, grp identities,
reassociation, rescoping). The rules are not confluent, so their order is
part of the semantics, and the two walks reproduce it:

1. The counting walk decides which seqs survive. Empty items vanish, and a
   scope with no surviving element vanishes with them. A seq is kept when
   it groups two or more compositions and is not directly under a seq; for
   that count a grp beneath it is opaque and a seq is transparent. Seq
   survival never depends on a grp decision.
2. The emitting walk lowers each run (empty terms dropped, the pads between
   survivors merged, the first survivor's wrappers kept), threads the pad
   between surviving neighbours of a spine (a vanished element's pads merge
   into the one composition that remains; a spine's leading and trailing
   pads drop, which is the reference discarding a forwarded pad at every
   wrapper), decides the grps, composes each surviving spine right-nested
   with the nest/pack wrappers its operands share (the chain of their
   paths' lowest common ancestor) factored out, and splices a
   dropped or absorbed scope's elements into the enclosing spine. A grp is
   absorbed when it is the first surviving element of a spine that is
   itself at the head of its group (a kept seq resets the head), and
   dropped when it groups no composition; for that count seqs and absorbed
   grps are transparent, kept grps opaque.

Right-nesting is native to the read-back, so reassociation is not a step;
it only existed because dropped wrappers spliced spines together.

### `Doc` and the renderer

`Doc` is one optional root object per line (`None` for an empty line), an
object arena, one text buffer (a run is one contiguous range of it, with
the spaces of its padded fixed compositions already joined), and one side
table of measures: each object's mid-line extent and its mid-line distance
to the first composition boundary. Mid-line, neither nest nor pack advances
the position, so both are exact state-independent sums over the children,
computed as each object is pushed (children always precede their parent).

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
`typeset::dsl` tokens (spans as positions), reads each string literal's
source text with the DSL's own string syntax, hands the tokens to the
shared token parser, and builds constructor calls through its builder
trait, so the macro and `typeset::dsl::parse` share one grammar
implementation and accept the same literals. A bare identifier is a
variable: a `Layout` in scope, cloned. Errors are spanned `compile_error!`
invocations. The crate depends on `proc-macro2` and `quote` alone; the
crate README is its documentation.

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
