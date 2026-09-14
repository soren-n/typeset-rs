(* ASCII only: the one deliberate divergence from the reference is that
   width counts characters where the reference counts bytes, so non-ASCII
   text is excluded here and covered by a Rust test. The quote, backslash
   and control characters are what exercises the DSL's string escapes in
   both front ends. *)
let word_chars =
  "0123456789" ^
  "abcdefghijklmnopqrstuvwxyz" ^
  "ABCDEFGHIJKLMNOPQRSTUVWXYZ" ^
  "[]{}#$%*+-.,/:;<>=?@ \"\\\n\t"

let word_char_gen state =
  let index = Random.State.int state (String.length word_chars) in
  String.get word_chars index

(* Length 0 included: the empty text is the empty layout, and both
   implementations must drop it wherever it lands. *)
let gen_word =
  let open QCheck.Gen in
  nat_small >>= fun n ->
  string_size ~gen:word_char_gen (0--(n + 1))

let shrink_word word yield =
  let length = String.length word in
  if length <= 1 then () else
  let last = length - 1 in
  for i = 0 to last do
    let word' = Bytes.init last
      (fun j -> if j<i then word.[j] else word.[j+1])
    in
    yield (Bytes.unsafe_to_string word')
  done

(* A DSL string literal: the DSL's escapes, every other character raw. *)
let print_word word =
  let buf = Buffer.create (String.length word + 2) in
  Buffer.add_char buf '"';
  String.iter
    (fun c ->
      match c with
      | '\\' -> Buffer.add_string buf "\\\\"
      | '"' -> Buffer.add_string buf "\\\""
      | '\n' -> Buffer.add_string buf "\\n"
      | '\r' -> Buffer.add_string buf "\\r"
      | '\t' -> Buffer.add_string buf "\\t"
      | '\000' -> Buffer.add_string buf "\\0"
      | c -> Buffer.add_char buf c)
    word;
  Buffer.add_char buf '"';
  Buffer.contents buf

let arbitrary_word =
  QCheck.make gen_word
    ~print:print_word
    ~shrink:shrink_word
