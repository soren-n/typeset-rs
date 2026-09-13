# typeset

An embedded DSL for defining source code pretty printers.

A layout is a tree of text literals joined by compositions that may or may
not break across lines, under wrappers that decide how a group of
compositions breaks and how continuation lines indent. The layout language
is designed to fit over a structurally recursive pass of the data you want
to print. Compiling a layout resolves the wrappers into a document;
rendering a document lays it out greedily at a tab width and a target line
width, fitting as many literals on each line as the wrappers allow.

```rust
use typeset::*;

let args = pack(seq(join_with_commas([text("x"), text("y"), text("z")])));
let call = unpad(text("f("), fix_unpad(args, text(")")));

let doc = call.compile();
assert_eq!(doc.render(2, 80), "f(x, y, z)");
assert_eq!(doc.render(2, 6), "f(x,\n  y,\n  z)");
```

The crate has no dependencies. The `typeset-parser` crate adds the same
language as a compile-time macro (see the end of this page).

## Literals and the empty layout

`text` is a literal, a single unit that never breaks. `null` is the empty
layout: it vanishes from the document, together with any wrapper on it, and
is neutral in every composition. It is the natural result for optional data
that is absent.

```rust
use typeset::*;

let foobar = unpad(text("foo"), unpad(null(), text("bar")));
assert_eq!(foobar.compile().render(2, 80), "foobar");
```

A literal wider than the target width still renders on one line; the width
is a target, not a limit.

## Compositions

`comp(left, right, Pad, Break)` joins two layouts. The `Pad` axis puts a
space between them when they share a line; the `Break` axis says whether the
composition may break, which moves `right` to the next line. The four
combinations have names:

| Shortcut   | DSL  | Space | Breaks |
|------------|------|-------|--------|
| `unpad`    | `&`  | no    | yes    |
| `pad`      | `+`  | yes   | yes    |
| `fix_unpad`| `!&` | no    | never  |
| `fix_pad`  | `!+` | yes   | never  |

`line(left, right)` always breaks.

```rust
use typeset::*;

assert_eq!(unpad(text("foo"), text("bar")).compile().render(2, 80), "foobar");
assert_eq!(pad(text("foo"), text("bar")).compile().render(2, 80), "foo bar");
assert_eq!(line(text("foo"), text("bar")).compile().render(2, 80), "foo\nbar");
// A blank line is a line break onto the empty layout.
assert_eq!(line(text("a"), line(null(), text("b"))).compile().render(2, 80), "a\n\nb");
```

A fixed composition binds the rightmost literal of its left operand to the
leftmost literal of its right operand; everything else in either operand
may still break. That is how punctuation attaches: a comma to the item
before it, a closing delimiter to the last thing it encloses. `"foo" &
("bar" !& "baz")` is the same layout as `"foo" & fix ("bar" & "baz")`:

```rust
use typeset::*;

let infix = unpad(text("foo"), fix_unpad(text("bar"), text("baz")));
let wrapped = unpad(text("foo"), fix(unpad(text("bar"), text("baz"))));
assert_eq!(infix.compile().render(2, 5), "foo\nbarbaz");
assert_eq!(wrapped.compile().render(2, 5), "foo\nbarbaz");
```

The literals a fixed composition binds form one unbreakable run, and that
run keeps the `nest`/`pack` wrappers of its *first* literal. So fix a
delimiter to what precedes it, and compose an opening delimiter with
`unpad`: `fix_unpad(text("("), pack(args))` would take the first argument
out of the pack.

## Wrappers

### fix

`fix` never breaks anything inside it: the layout is treated as a literal.

```rust
use typeset::*;

let foobar = fix(unpad(text("foo"), text("bar")));
assert_eq!(foobar.compile().render(2, 2), "foobar");
```

### grp

`grp` keeps its compositions from breaking as long as a composition to its
left can break instead: from the outside the group is measured as a block.
When the group itself does not fit, its compositions break.

```rust
use typeset::*;

let grouped = unpad(text("foo"), grp(unpad(text("bar"), text("baz")))).compile();
assert_eq!(grouped.render(2, 10), "foobarbaz");
assert_eq!(grouped.render(2, 7), "foo\nbarbaz");
assert_eq!(grouped.render(2, 4), "foo\nbar\nbaz");

// Without the group the greedy solver fills the first line instead.
let plain = unpad(text("foo"), unpad(text("bar"), text("baz")));
assert_eq!(plain.compile().render(2, 7), "foobar\nbaz");
```

### seq

`seq` breaks every one of its compositions as soon as one of them breaks:
once one item of a list goes on a new line, they all do. A sequence that
does not fit breaks every composition beneath it, nested sequences
included; only a `grp` lets nested content fit on its own.

```rust
use typeset::*;

let items = seq(unpad(text("foo"), unpad(text("bar"), text("baz"))));
assert_eq!(items.compile().render(2, 7), "foo\nbar\nbaz");

let list = pad(text("x"), seq(join_with_spaces([text("aa"), text("bb"), text("cc")])));
assert_eq!(list.compile().render(2, 8), "x aa\nbb\ncc");
```

### nest

`nest` indents every line its content breaks onto by one tab, the first
argument of `render`.

```rust
use typeset::*;

let nested = unpad(text("foo"), nest(unpad(text("bar"), text("baz")))).compile();
assert_eq!(nested.render(2, 7), "foobar\n  baz");
assert_eq!(nested.render(2, 4), "foo\n  bar\n  baz");
```

### pack

`pack` aligns the lines its content breaks onto to the column where the
content started: hanging indentation, as in a Lisp call whose arguments
line up under the first one. The column is `max(indentation, mark)`, so a
mark never pulls a line left of its indentation.

```rust
use typeset::*;

let packed = unpad(text("foo"), pack(unpad(text("bar"), text("baz")))).compile();
assert_eq!(packed.render(2, 7), "foobar\n   baz");
assert_eq!(packed.render(2, 4), "foo\nbar\nbaz");
```

## Joins

`join_with_spaces`, `join_with_commas` and `join_with_lines` fold a
collection with `pad`, a comma fixed to each item followed by `pad`, and
`line` respectively. An empty collection is `null`.

```rust
use typeset::*;

let doc = join_with_commas([text("x"), text("y"), text("z")]).compile();
assert_eq!(doc.render(2, 80), "x, y, z");
assert_eq!(doc.render(2, 3), "x,\ny,\nz");
```

## Compile, then render

`Layout::compile` is infallible and runs in constant native stack: a
layout of any depth compiles, with depth costing heap. `Doc::render` only
borrows the document, so one compiled document renders at several widths,
which is what a buffer of variable width needs. Break decisions are O(1),
so rendering does not slow down with the target width. Width is counted in
characters.

```rust
use typeset::*;

let doc = pad(text("This"), pad(text("is"), pad(text("a"), text("test")))).compile();
assert_eq!(doc.render(2, 100), "This is a test");
assert_eq!(doc.render(2, 5), "This\nis a\ntest");
```

## The DSL

The same language as a string, parsed at run time by `typeset::dsl::parse`,
or as a compile-time macro from the `typeset-parser` crate, where a bare
identifier names a `Layout` in scope:

```text
null      the empty layout
"x"       a literal
fix u     grp u     seq u     nest u     pack u
u & v     u + v     u !& v    u !+ v     u @ v     u @@ v
```

All binary operators share one precedence level and associate to the
right; parenthesize for any other grouping.

```rust
let layout = typeset::dsl::parse(r#"nest ("function" + "name()") @ "{ body }""#)?;
assert_eq!(layout.compile().render(2, 40), "  function name()\n{ body }");
# Ok::<(), typeset::dsl::ParseError>(())
```
