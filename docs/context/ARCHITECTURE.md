# Architecture

## Project structure

A Rust workspace of three crates:
- **typeset**: the layout language, compiler, renderer, and the DSL parser
  (`typeset::dsl`)
- **typeset-macro**: the `layout!` procedural macro, a token adapter over
  `typeset::dsl`
- **oracle/driver** (`typeset-oracle-driver`, unpublished): the process the
  OCaml oracle harness renders through

## typeset crate (`typeset/src/`)

- `lib.rs`: the crate doc (the crate README), the public API (`Layout`,
  `Doc`, `Pad`, `Break`, the constructors) and `Layout::compile`
- `constructors.rs`: the functions users build layouts with
- `layout.rs`: `Layout`, the public input type
- `dsl.rs`, `dsl/grammar.rs`: the DSL: the string syntax, the printer
  behind `Layout`'s and `Doc`'s DSL output, the run-time front end
  (`Layout`'s `FromStr`), and the hidden grammar module the macro feeds
- `arena.rs`: the arena primitives every representation is built from
- `lines.rs`, `graph.rs`, `emit.rs`: the compiler; `lines` owns the line
  it lends, `graph` the scope graph it solves, `emit` the `Doc` it fills
- `doc.rs`: `Doc`, the public output type, which measures objects as they
  are pushed
- `render.rs`: `Doc::render`

### Arenas, ids, ranges, trees

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
  wrapper chains in `lines` are trees.

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
(balanced) case. `Clone` and `Drop` derive; cloning is two allocations.
`Display` prints the DSL form, by an explicit stack, and `FromStr` parses
it back to the same layout; `Debug` is `Display`.

### Pipeline

| Pass    | Lowers                    | Does |
|---------|---------------------------|------|
| `lines` | `Layout` → lines of events | one DFS, paused at each hard line: split into lines at hard breaks and inside broken sequences; mark each composition fixed or breakable; before each composition, the scopes that close and open |
| `graph` | a line → its scope graph  | read the items (runs of fixed-joined texts) off the events, build the grp/seq scope graph, solve the runs that straddle a scope boundary |
| `emit`  | solved lines → `Doc`      | two walks over a stack of open spines: drop empty texts, decide the grp/seq identities, right-nest every spine, factor shared nest/pack wrappers, measure every object |

Nothing crosses a hard line, so the passes are a pipeline of lines:
`Lines` lends one line at a time and `Emitter` consumes it with scratch
reused across lines. The intermediate is one line, whatever the document's
size. Each module's doc comment is the description of its pass: the
representation it owns, the invariants it keeps, and which of the
reference's rewrites it performs. The reference runs the emitter's rules
as five separate tree rewrites in a fixed order (null removal, seq
identities, grp identities, reassociation, rescoping); the rules are not
confluent, so the order is part of the semantics and the two walks
reproduce it.

### `Doc` and the renderer

`Doc` is one optional root object per line (`None` for an empty line), an
object arena, and one text buffer (a run is one contiguous range of it,
with the spaces of its padded fixed compositions already joined). An
object carries its node and its measures: its mid-line extent and its
mid-line distance to the first composition boundary. Mid-line, neither
nest nor pack advances the position, so both are exact state-independent
sums over the children, computed as each object is pushed (children
always precede their parent). `Debug` prints the document in the DSL,
through the same iterative printer as `Layout`; the document is a normal
form, so that DSL compiles to the same document.

The renderer walks the arena with an explicit frame stack. `should_break`
is arithmetic on the boundary measure. `will_fit` is arithmetic on the
extent measure mid-line; at the head of a line indentation offsets depend on the
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

## typeset-macro crate (`typeset-macro/src/`)

`lib.rs`: the `layout!` macro. It flattens its Rust token trees into
`typeset::dsl` tokens (spans as positions), reads each string literal's
source text with the DSL's own string syntax, hands the tokens to the
shared token parser, and builds constructor calls through its builder
trait, so the macro and `Layout`'s `FromStr` share one grammar
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
