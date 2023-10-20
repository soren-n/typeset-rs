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
    | UFix layout1 -> _visit layout1 _group |> sprintf "fix %s"
    | UGrp layout1 -> _visit layout1 _group |> sprintf "grp %s"
    | USeq layout1 -> _visit layout1 _group |> sprintf "seq %s"
    | UNest layout1 -> _visit layout1 _group |> sprintf "nest %s"
    | UPack layout1 -> _visit layout1 _group |> sprintf "pack %s"
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

let run cmd =
  let channel = Unix.open_process_in cmd in
  let result = In_channel.input_lines channel in
  In_channel.close channel;
  _process_output result []

let rust_impl layout_dsl =
  let open Printf in
  run (sprintf "./_build/unit '%s'" layout_dsl)

let rust_ocaml_identity =
  QCheck.Test.make ~count: 1024
    ~name: "rust_ocaml_identity"
    arbitrary_eDSL
    (fun layout ->
      let open Printf in
      print_layout layout |> fun layout_dsl ->
      compile layout |> fun document ->
      render document 2 80 |> fun expected_output ->
      rust_impl layout_dsl |> fun maybe_actual_output ->
      match maybe_actual_output with
      | None -> assert false
      | Some (log, actual_output) ->
        let judgement = expected_output = actual_output in
        if judgement then true else begin
        printf "============= log ================\n";
        printf "%s\n" (String.concat "\n" log);
        printf "============ layout ==============\n";
        printf "%s\n" layout_dsl;
        printf "======== expected_output =========\n";
        printf "\"%s\"\n" expected_output;
        printf "========= actual_output ==========\n";
        printf "\"%s\"\n" actual_output;
        printf "============== end ===============\n";
        false
        end)

let _ =
  QCheck_runner.run_tests
  [ rust_ocaml_identity
  ]