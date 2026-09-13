# Examples

Three formatters, each a page long, showing the idioms the layout language
is built around:

- `json_formatter.rs`: containers that open and close on their own lines
  when broken (`seq` over the delimiters and entries, `grp` per container,
  `nest` for the entries, a key fixed to its value)
- `lisp_formatter.rs`: arguments aligned under the first argument (`pack`),
  every argument on its own line once one breaks (`seq`), each call fitting
  on its own (`grp`)
- `code_formatter.rs`: braces on their own lines (`line`), calls with
  aligned arguments, keywords fixed to their parentheses, operators that
  break as a unit

```bash
cargo run --example json_formatter -p typeset
```

The macro form of the same language is in `typeset-parser/examples/full.rs`.
The profiling probes live in `benches/`; see
[docs/context/PERFORMANCE.md](../../docs/context/PERFORMANCE.md).
