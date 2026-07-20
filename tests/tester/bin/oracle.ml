(* One-off oracle: parse a layout DSL string and render it with the OCaml
   reference implementation. Mirrors the grammar in tests/unit/src/layout.pest
   (all binary operators share one precedence level and associate right). *)

open Typeset

exception Parse_error of string

type token =
  | TLParen
  | TRParen
  | TIdent of string
  | TString of string
  | TOp of string

let tokenize input =
  let n = String.length input in
  let rec skip i = if i < n && (input.[i] = ' ' || input.[i] = '\t' || input.[i] = '\n') then skip (i + 1) else i in
  let rec go i acc =
    let i = skip i in
    if i >= n then List.rev acc else
    match input.[i] with
    | '(' -> go (i + 1) (TLParen :: acc)
    | ')' -> go (i + 1) (TRParen :: acc)
    | '"' ->
      let buf = Buffer.create 16 in
      let rec str j =
        if j >= n then raise (Parse_error "unterminated string") else
        match input.[j] with
        | '"' -> j + 1
        | '\\' ->
          if j + 1 >= n then raise (Parse_error "dangling escape") else
          let c = match input.[j + 1] with
            | 'n' -> '\n' | 'r' -> '\r' | 't' -> '\t' | '0' -> '\000'
            | '\\' -> '\\' | '"' -> '"' | '\'' -> '\''
            | c -> raise (Parse_error (Printf.sprintf "bad escape \\%c" c))
          in
          Buffer.add_char buf c; str (j + 2)
        | c -> Buffer.add_char buf c; str (j + 1)
      in
      let j = str (i + 1) in
      go j (TString (Buffer.contents buf) :: acc)
    | c when (c >= 'a' && c <= 'z') ->
      let j = ref i in
      while !j < n && input.[!j] >= 'a' && input.[!j] <= 'z' do incr j done;
      go !j (TIdent (String.sub input i (!j - i)) :: acc)
    | _ ->
      (* Longest-match first, so "!&" and "@@" are not split. *)
      let two = if i + 1 < n then String.sub input i 2 else "" in
      if two = "@@" || two = "!&" || two = "!+" then go (i + 2) (TOp two :: acc)
      else
        let one = String.make 1 input.[i] in
        if one = "@" || one = "&" || one = "+" then go (i + 1) (TOp one :: acc)
        else raise (Parse_error (Printf.sprintf "unexpected character %c" input.[i]))
  in
  go 0 []

let rec parse_expr tokens =
  let (left, rest) = parse_atom tokens in
  match rest with
  | TOp op :: rest1 ->
    let (right, rest2) = parse_expr rest1 in
    let node = match op with
      | "@" -> ULine (left, right)
      | "@@" -> ULine (left, ULine (UNull, right))
      | "&" -> UComp (left, right, { pad = false; fix = false })
      | "+" -> UComp (left, right, { pad = true; fix = false })
      | "!&" -> UComp (left, right, { pad = false; fix = true })
      | "!+" -> UComp (left, right, { pad = true; fix = true })
      | _ -> raise (Parse_error ("unknown operator " ^ op))
    in
    (node, rest2)
  | _ -> (left, rest)

and parse_atom tokens =
  match tokens with
  | TIdent "null" :: rest -> (UNull, rest)
  | TIdent id :: rest ->
    let (inner, rest1) = parse_primary rest in
    let node = match id with
      | "fix" -> UFix inner
      | "grp" -> UGrp inner
      | "seq" -> USeq inner
      | "nest" -> UNest inner
      | "pack" -> UPack inner
      | _ -> raise (Parse_error ("unknown prefix operator " ^ id))
    in
    (node, rest1)
  | _ -> parse_primary tokens

and parse_primary tokens =
  match tokens with
  | TString data :: rest -> (UText data, rest)
  | TIdent "null" :: rest -> (UNull, rest)
  | TLParen :: rest ->
    let (inner, rest1) = parse_expr rest in
    (match rest1 with
     | TRParen :: rest2 -> (inner, rest2)
     | _ -> raise (Parse_error "expected )"))
  | _ -> raise (Parse_error "expected a primary expression")

let parse input =
  match parse_expr (tokenize input) with
  | (layout, []) -> layout
  | (_, _) -> raise (Parse_error "trailing tokens")

let () =
  if Array.length Sys.argv < 2 then begin
    prerr_endline "usage: oracle '<layout dsl>' [tab] [width]";
    exit 2
  end;
  let tab = if Array.length Sys.argv > 2 then int_of_string Sys.argv.(2) else 2 in
  let width = if Array.length Sys.argv > 3 then int_of_string Sys.argv.(3) else 80 in
  match parse Sys.argv.(1) with
  | layout -> print_string (render (compile layout) tab width); print_newline ()
  | exception Parse_error msg -> prerr_endline ("parse error: " ^ msg); exit 2
