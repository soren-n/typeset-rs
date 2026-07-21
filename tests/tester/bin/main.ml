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

let rec _process_output items log =
  match items with
  | [] -> None
  | "!!!!output!!!!" :: output ->
    Some (List.rev log, (String.concat "\n" output))
  | item :: items1 ->
    _process_output items1 (item :: log)

(* Must close with Unix.close_process_in, not In_channel.close: the latter
   closes the descriptor without reaping the child, so one zombie accumulates
   per generated case until fork fails with EAGAIN. *)
let run cmd =
  let channel = Unix.open_process_in cmd in
  let result = In_channel.input_lines channel in
  ignore (Unix.close_process_in channel);
  _process_output result []

let rust_impl layout_dsl tab width =
  let open Printf in
  run (sprintf "./_build/unit '%s' %d %d" layout_dsl tab width)

(* Each case pairs a layout with a (tab, width) to render at. Fixing the
   dimensions per case (rather than always 2/80) is what exercises the breaking
   decisions structurize drives: the width is biased narrow, where grp/seq
   scopes actually differ. tab/width are part of the case so shrinking keeps
   the failing dimensions fixed while it minimizes the layout. *)
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
  QCheck.Test.make ~count: 2048
    ~name: "rust_ocaml_identity"
    arbitrary_case
    (fun (layout, tab, width) ->
      let open Printf in
      print_layout layout |> fun layout_dsl ->
      compile layout |> fun document ->
      render document tab width |> fun expected_output ->
      rust_impl layout_dsl tab width |> fun maybe_actual_output ->
      match maybe_actual_output with
      | None -> assert false
      | Some (rust_log, actual_output) ->
        let judgement = expected_output = actual_output in
        if judgement then true else begin
        printf "============ layout (tab=%d width=%d) ==============\n" tab width;
        printf "%s\n" layout_dsl;
        printf "======== expected_output =========\n";
        printf "\"%s\"\n" expected_output;
        printf "========= actual_output ==========\n";
        printf "\"%s\"\n" actual_output;
        printf "=========== rust log =============\n";
        printf "%s\n" (String.concat "\n" rust_log);
        printf "============== end ===============\n";
        false
        end)

(* Propagate the runner's status: discarding it made the executable exit 0 even
   when a property failed, so no caller could detect a failure. *)
let () =
  exit (QCheck_runner.run_tests [ rust_ocaml_identity ])