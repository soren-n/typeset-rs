(* The oracle harness. With no arguments, the QCheck identity suite: for
   generated layouts, tabs and widths, the reference's rendering equals the
   Rust driver's. With a layout DSL string (and optionally a tab and width),
   one case rendered by both implementations. With --reference, the
   reference's rendering alone, as the `|` lines of a pinned case. *)

open Typeset
open EDSL

let print_layout layout =
  let open Printf in
  let _skip dsl = dsl in
  let _group dsl = sprintf "(%s)" dsl in
  let rec _visit layout wrap =
    match layout with
    | UNull -> "null"
    | UText data -> sprintf "\"%s\"" data
    | UFix layout1 ->
      _visit layout1 _group |> fun dsl ->
      wrap (sprintf "fix %s" dsl)
    | UGrp layout1 ->
      _visit layout1 _group |> fun dsl ->
      wrap (sprintf "grp %s" dsl)
    | USeq layout1 ->
      _visit layout1 _group |> fun dsl ->
      wrap (sprintf "seq %s" dsl)
    | UNest layout1 ->
      _visit layout1 _group |> fun dsl ->
      wrap (sprintf "nest %s" dsl)
    | UPack layout1 ->
      _visit layout1 _group |> fun dsl ->
      wrap (sprintf "pack %s" dsl)
    | ULine (left, right) ->
      _visit left _skip |> fun left1 ->
      _visit right _group |> fun right1 ->
      wrap (sprintf "%s @ %s" left1 right1)
    | UComp (left, right, attr) ->
      _visit left _skip |> fun left1 ->
      _visit right _group |> fun right1 ->
      match attr.pad, attr.fix with
      | false, false -> wrap (sprintf "%s & %s" left1 right1)
      | false, true -> wrap (sprintf "%s !& %s" left1 right1)
      | true, false -> wrap (sprintf "%s + %s" left1 right1)
      | true, true -> wrap (sprintf "%s !+ %s" left1 right1)
  in
  _visit layout _skip

(* The Rust driver: one process for the whole run. A request is one line,
   `tab width dsl`; the reply is `ok bytes` or `error bytes` on a line, then
   exactly that many bytes. *)
let driver = lazy (Unix.open_process_args "./_build/driver" [| "driver" |])

let rust_impl layout_dsl tab width =
  let (reply, request) = Lazy.force driver in
  Printf.fprintf request "%d %d %s\n" tab width layout_dsl;
  flush request;
  let header = input_line reply in
  match String.split_on_char ' ' header with
  | [ "ok"; bytes ] -> really_input_string reply (int_of_string bytes)
  | [ "error"; bytes ] ->
    failwith ("driver: " ^ really_input_string reply (int_of_string bytes))
  | _ -> failwith ("driver: unexpected reply " ^ header)

let ocaml_impl layout tab width = render (compile layout) tab width

(* Each case pairs a layout with a (tab, width) to render at. Fixing the
   dimensions per case (rather than always 2/80) is what exercises the breaking
   decisions: the width is biased narrow, where grp/seq scopes actually differ.
   tab/width are part of the case so shrinking keeps the failing dimensions
   fixed while it minimizes the layout. *)
let gen_case =
  let open QCheck.Gen in
  gen_eDSL >>= fun layout ->
  oneof_list [0; 1; 2; 4; 8] >>= fun tab ->
  oneof_weighted
    [ 4, oneof_list [1; 2; 3; 4; 5; 6; 8]
    ; 2, oneof_list [10; 12; 16; 20]
    ; 1, oneof_list [40; 80] ]
  >>= fun width ->
  return (layout, tab, width)

let arbitrary_case =
  let print (layout, tab, width) =
    Printf.sprintf "%s   [tab=%d width=%d]" (print_layout layout) tab width
  in
  let shrink (layout, tab, width) =
    QCheck.Iter.map (fun layout1 -> (layout1, tab, width)) (shrink_eDSL layout)
  in
  QCheck.make gen_case ~print ~shrink

let rust_ocaml_identity =
  QCheck.Test.make ~count: 20000
    ~name: "rust_ocaml_identity"
    arbitrary_case
    (fun (layout, tab, width) ->
      let open Printf in
      let layout_dsl = print_layout layout in
      let expected_output = ocaml_impl layout tab width in
      let actual_output = rust_impl layout_dsl tab width in
      if expected_output = actual_output then true else begin
        printf "============ layout (tab=%d width=%d) ==============\n" tab width;
        printf "%s\n" layout_dsl;
        printf "======== expected_output =========\n";
        printf "\"%s\"\n" expected_output;
        printf "========= actual_output ==========\n";
        printf "\"%s\"\n" actual_output;
        printf "============== end ===============\n";
        false
      end)

let parse_or_exit layout_dsl =
  try Parse.parse layout_dsl with
  | Parse.Parse_error message ->
    prerr_endline ("parse error: " ^ message); exit 2

let reference layout_dsl tab width =
  ocaml_impl (parse_or_exit layout_dsl) tab width
  |> String.split_on_char '\n'
  |> List.iter (fun line -> print_endline ("|" ^ line))

let compare_one layout_dsl tab width =
  let layout = parse_or_exit layout_dsl in
  let expected = ocaml_impl layout tab width in
  let actual = rust_impl layout_dsl tab width in
  if expected = actual then begin
    Printf.printf "MATCH: %s\n%s\n" layout_dsl expected; 0
  end else begin
    Printf.printf "DIFF:  %s\n--- ocaml ---\n%s\n--- rust ---\n%s\n" layout_dsl expected actual; 1
  end

(* Propagate the runner's status: discarding it made the executable exit 0 even
   when a property failed, so no caller could detect a failure. *)
let () =
  match List.tl (Array.to_list Sys.argv) with
  | [] -> exit (QCheck_runner.run_tests [ rust_ocaml_identity ])
  | [ layout_dsl ] -> exit (compare_one layout_dsl 2 80)
  | [ layout_dsl; tab; width ] ->
    exit (compare_one layout_dsl (int_of_string tab) (int_of_string width))
  | [ "--reference"; layout_dsl; tab; width ] ->
    reference layout_dsl (int_of_string tab) (int_of_string width)
  | _ ->
    prerr_endline "usage: tester                          the identity suite";
    prerr_endline "       tester '<layout dsl>' [tab width] one case, both implementations";
    prerr_endline "       tester --reference '<layout dsl>' tab width";
    prerr_endline "                                        the reference's rendering, as a pinned case";
    exit 2
