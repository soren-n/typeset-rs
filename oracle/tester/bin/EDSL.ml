open Typeset
open Word

let _null = UNull
let _text data = UText data
let _fix eDSL = UFix eDSL
let _seq eDSL = USeq eDSL
let _grp eDSL = UGrp eDSL
let _nest eDSL = UNest eDSL
let _pack eDSL = UPack eDSL
let _line left right = ULine (left, right)
let _comp left right attr = UComp (left, right, attr)

let rec _gen_eDSL n =
  let open QCheck.Gen in
  let _gen_eDSL_term =
    oneof
    [ return _null
    ; map _text gen_word
    ]
  in
  if n <= 0 then _gen_eDSL_term else
  (* Biased toward grp/seq: stacked scopes are where the breaking decisions
     of the two implementations can diverge, and a uniform pick almost never
     produces them (the historical grp(seq(x)) ordering bug survived fifteen
     clean runs of a uniform generator). *)
  let _gen_eDSL_unary =
    oneof_weighted
    [ 7, (fun st -> map _grp (_gen_eDSL (n / 2)) st)
    ; 7, (fun st -> map _seq (_gen_eDSL (n / 2)) st)
    ; 2, (fun st -> map _fix (_gen_eDSL (n / 2)) st)
    ; 2, (fun st -> map _nest (_gen_eDSL (n / 2)) st)
    ; 2, (fun st -> map _pack (_gen_eDSL (n / 2)) st)
    ]
  in
  let _gen_eDSL_line st =
    map2 _line
      (_gen_eDSL (n / 2))
      (_gen_eDSL (n / 2))
      st
  in
  let _gen_attr =
    bool >>= fun pad ->
    bool >>= fun fix ->
    return { pad = pad; fix = fix }
  in
  let _gen_eDSL_comp =
    _gen_attr >>= fun attr ->
    map2 (fun left right -> _comp left right attr)
      (_gen_eDSL (n / 2))
      (_gen_eDSL (n / 2))
  in
  let _gen_eDSL_binary =
    oneof_weighted
    [ 1, _gen_eDSL_line
    ; 2, _gen_eDSL_comp
    ]
  in
  oneof_weighted
  [ 2, _gen_eDSL_unary
  ; 3, _gen_eDSL_binary
  ]

let gen_eDSL =
  let open QCheck.Gen in
  nat_small >>= _gen_eDSL

let shrink_eDSL eDSL =
  let open QCheck.Iter in
  let rec _shrink eDSL =
    match eDSL with
    | UNull -> empty
    | UText data ->
      empty <+> (shrink_word data >|= _text)
    | UFix eDSL1 ->
      return eDSL1
      <+> (_shrink eDSL1 >|= _fix)
    | USeq eDSL1 ->
      return eDSL1
      <+> (_shrink eDSL1 >|= _seq)
    | UGrp eDSL1 ->
      return eDSL1
      <+> (_shrink eDSL1 >|= _grp)
    | UNest eDSL1 ->
      return eDSL1
      <+> (_shrink eDSL1 >|= _nest)
    | UPack eDSL1 ->
      return eDSL1
      <+> (_shrink eDSL1 >|= _pack)
    | ULine (left, right) ->
      of_list [left; right]
      <+> (_shrink left >|= fun left1 -> _line left1 right)
      <+> (_shrink right >|= fun right1 -> _line left right1)
    | UComp (left, right, attr) ->
      _shrink_attr attr >>= fun attr1 ->
      of_list [left; right]
      <+> (_shrink left >>= fun left1 ->
        (return (_comp left1 right attr1)))
      <+> (_shrink right >>= fun right1 ->
        (return (_comp left right1 attr1)))
  and _shrink_attr attr =
    return attr <+>
    return { attr with pad = not attr.pad } <+>
    return { attr with fix = not attr.fix } <+>
    return { pad = not attr.pad; fix = not attr.fix }
  in
  _shrink eDSL
