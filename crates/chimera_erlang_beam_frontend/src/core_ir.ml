(* chimera_erlang_beam_frontend - Core Erlang IR *)
(* Core Erlang intermediate representation *)

module Ast = struct
  (* Source AST types *)
  type expr =
    | Atom of string
    | Integer of int
    | Float of float
    | String of string
    | Var of string
    | BinOp of expr * binop * expr
    | Fun of string * expr
    | Case of expr * clause list * expr
    | Receive of clause list * int * expr
    | Call of expr * expr list
    | Let of string * expr * expr
    | Seq of expr * expr
    | List of expr list
    | Tuple of expr list
    | BitString of expr list
    | BitSeg of bitseg
    | Not of expr
    | AndAlso of expr * expr list
    | OrElse of expr * expr list
    | Try of expr * clause list * expr * (string * expr) list
    | Throw of expr
    | True
    | False
    | Nil

  and binop = Add | Sub | Mul | Div | Eq | Neq | Lt | Gt | Lte | Gte | Cons | AndAlso | OrElse

  and clause = {
    pattern: pattern;
    guard: expr;
    body: expr;
  }

  and pattern =
    | PAtom of string
    | PInteger of int
    | PVar of string
    | PList of pattern list
    | PTuple of pattern list
    | PCons of pattern * pattern
    | PBitString of patseg list

  and bitseg = {
    expr: expr;
    size: expr option;
    opts: bitopt list;
  }

  and patseg = {
    pattern: pattern;
    size: expr option;
    opts: bitopt list;
  }

  and bitopt =
    | BitBinary
    | BitInteger
    | BitFloat
    | BitNative
    | BitBig
    | BitLittle
end

module Core = struct
  (* Core Erlang types after lowering *)

  type literal =
    | LitAtom of string
    | LitInt of int
    | LitFloat of float
    | LitString of string
    | LitNil
    | LitCons of literal * literal

  type pattern =
    | CPAtom of string
    | CPInt of int
    | CPVar of string
    | CPList of pattern list
    | CPTuple of pattern list

  type expr =
    | CVar of string
    | CLit of literal
    | CFun of string * expr
    | CApp of expr * expr list
    | CLet of string * expr * expr
    | CSeq of expr * expr
    | CCase of expr * core_clause list * expr
    | CReceive of core_clause list * int * expr
    | CPrimOp of string * expr list
    | CExternal of string * string * expr list
    | CTrue
    | CFalse
    | CNil
    | CCons of expr * expr
    | CTuple of expr list
    | CBitString of expr list
    | CNot of expr
    | CAndAlso of expr * expr
    | COrElse of expr * expr
    | CTry of expr * core_clause list * expr * (string * expr) list
    | CThrow of expr

  and core_clause = {
    cp_pattern: pattern;
    cp_guard: expr;
    cp_body: expr;
  }

  (* Lowering from source AST to Core Erlang *)
  let rec lower_expr (e : Ast.expr) : expr =
    match e with
    | Ast.Atom s -> CLit (LitAtom s)
    | Ast.Integer i -> CLit (LitInt i)
    | Ast.Float f -> CLit (LitFloat f)
    | Ast.String s -> CLit (LitString s)
    | Ast.Var v -> CVar v
    | Ast.True -> CTrue
    | Ast.False -> CFalse
    | Ast.Nil -> CNil
    | Ast.BinOp (e1, op, e2) -> lower_binop e1 op e2
    | Ast.Fun (v, body) -> CFun (v, lower_expr body)
    | Ast.Case (e, clauses, body) ->
        CCase (lower_expr e, List.map lower_clause clauses, lower_expr body)
    | Ast.Receive (clauses, timeout, body) ->
        CReceive (List.map lower_clause clauses, timeout, lower_expr body)
    | Ast.Call (f, args) -> CApp (lower_expr f, List.map lower_expr args)
    | Ast.Let (v, e1, e2) -> CLet (v, lower_expr e1, lower_expr e2)
    | Ast.Seq (e1, e2) -> CSeq (lower_expr e1, lower_expr e2)
    | Ast.List es -> lower_list es
    | Ast.Tuple es -> CTuple (List.map lower_expr es)
    | Ast.BitString es -> CBitString (List.map lower_expr es)
    | Ast.BitSeg seg -> lower_bitseg seg
    | Ast.Not e -> CNot (lower_expr e)
    | Ast.AndAlso (e, es) -> lower_andalso e es
    | Ast.OrElse (e, es) -> lower_orelse e es
    | Ast.Try (e, clauses, body, traps) -> lower_try e clauses body traps
    | Ast.Throw e -> CThrow (lower_expr e)

  and lower_list [] = CNil
    | lower_list (h :: t) = CCons (lower_expr h, lower_list t)

  and lower_bitseg seg =
    let expr = lower_expr seg.Ast.expr in
    let size = match seg.Ast.size with
      | None -> CLit (LitInt (-1))
      | Some s -> lower_expr s in
    CPrimOp ("bs_init", [expr; size])

  and lower_andalso e [] = lower_expr e
    | lower_andalso e (h :: t) ->
      let e' = lower_expr e in
      let h' = lower_expr h in
      CPrimOp ("andalso", [e'; h'])

  and lower_orelse e [] = lower_expr e
    | lower_orelse e (h :: t) ->
      let e' = lower_expr e in
      let h' = lower_expr h in
      CPrimOp ("orelse", [e'; h'])

  and lower_try e clauses body traps =
    let e' = lower_expr e in
    let clauses' = List.map lower_clause clauses in
    let body' = lower_expr body in
    let traps' = List.map (fun (v, ex) -> (v, lower_expr ex)) traps in
    CTry (e', clauses', body', traps')

  and lower_binop e1 op e2 =
    let e1' = lower_expr e1 in
    let e2' = lower_expr e2 in
    match op with
    | Ast.Add -> CPrimOp ("+", [e1'; e2'])
    | Ast.Sub -> CPrimOp ("-", [e1'; e2'])
    | Ast.Mul -> CPrimOp ("*", [e1'; e2'])
    | Ast.Div -> CPrimOp ("/", [e1'; e2'])
    | Ast.Eq -> CPrimOp ("=:=", [e1'; e2'])
    | Ast.Neq -> CPrimOp ("=/=", [e1'; e2'])
    | Ast.Lt -> CPrimOp ("<", [e1'; e2'])
    | Ast.Gt -> CPrimOp (">", [e1'; e2'])
    | Ast.Lte -> CPrimOp ("=<", [e1'; e2'])
    | Ast.Gte -> CPrimOp (">=", [e1'; e2'])
    | Ast.Cons -> CCons (e1', e2')
    | Ast.AndAlso -> CPrimOp ("andalso", [e1'; e2'])
    | Ast.OrElse -> CPrimOp ("orelse", [e1'; e2'])

  and lower_clause (c : Ast.clause) : core_clause =
    {
      cp_pattern = lower_pattern c.Ast.pattern;
      cp_guard = lower_expr c.Ast.guard;
      cp_body = lower_expr c.Ast.body;
    }

  and lower_pattern (p : Ast.pattern) : pattern =
    match p with
    | Ast.PAtom s -> CPAtom s
    | Ast.PInteger i -> CPInt i
    | Ast.PVar v -> CPVar v
    | Ast.PList ps -> CPList (List.map lower_pattern ps)
    | Ast.PTuple ps -> CPTuple (List.map lower_pattern ps)
    | Ast.PCons (p1, p2) -> CPList [lower_pattern p1; lower_pattern p2]
    | Ast.PBitString pss -> CPList (List.map lower_patseg pss)

  and lower_patseg (ps : Ast.patseg) : pattern =
    CPVar ("__patseg")

  and string_of_binop = function
    | Ast.Add -> "+"
    | Ast.Sub -> "-"
    | Ast.Mul -> "*"
    | Ast.Div -> "/"
    | Ast.Eq -> "=:="
    | Ast.Neq -> "=/="
    | Ast.Lt -> "<"
    | Ast.Gt -> ">"
    | Ast.Lte -> "=<"
    | Ast.Gte -> ">="
    | Ast.Cons -> "|"
    | Ast.AndAlso -> "andalso"
    | Ast.OrElse -> "orelse"
end