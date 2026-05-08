(* chimera_erlang_beam_frontend - BEAM Bytecode Generator *)
(* Generates BEAM bytecode from Core Erlang IR *)

module Beam = struct
  (* BEAM opcode definitions *)
  type opcode =
    | OpMove
    | OpCall
    | OpCallExt
    | OpReturn
    | OpAdd
    | OpSub
    | OpMul
    | OpDiv
    | OpCmp
    | OpJmp
    | OpJe
    | OpJne
    | OpHalt
    | OpNop

  (* BEAM instruction representation *)
  type instruction = {
    op: opcode;
    args: int list;
  }

  (* Function code *)
  type function_code = {
    name: string;
    arity: int;
    code: instruction list;
    labels: (string, int) Hashtbl.t;
  }

  (* Module representation *)
  type module_ = {
    name: string;
    exports: string list;
    functions: function_code list;
  }

  (* Generate a label *)
  let make_label name = "L_" ^ name

  (* Opcode to bytes *)
  let opcode_to_bytes op =
    match op with
    | OpMove -> [| 0x00; 0x01 |]
    | OpCall -> [| 0x01 |]
    | OpCallExt -> [| 0x01; 0x00 |]
    | OpReturn -> [| 0x02 |]
    | OpAdd -> [| 0x03 |]
    | OpSub -> [| 0x04 |]
    | OpMul -> [| 0x05 |]
    | OpDiv -> [| 0x06 |]
    | OpCmp -> [| 0x07 |]
    | OpJmp -> [| 0x08 |]
    | OpJe -> [| 0x09 |]
    | OpJne -> [| 0x0A |]
    | OpHalt -> [| 0xFF |]
    | OpNop -> [| 0x00; 0x00 |]

  (* Generate function from Core *)
  let rec generate_function (name: string) (arity: int) (body: Core.expr) : function_code =
    let labels = Hashtbl.create 16 in
    let code = ref [] in
    let label_counter = ref 0 in

    let next_label () =
      let l = !label_counter in
      label_counter := l + 1;
      make_label (string_of_int l)
    in

    let emit inst = code := inst :: !code in

    let rec gen_expr (e: Core.expr) =
      match e with
      | Core.CVar v ->
        (* Variable - load from X register *)
        emit { op = OpMove; args = [0] }  (* placeholder *)
      | Core.CLit lit ->
        (* Literal - load immediate *)
        (match lit with
         | Core.LitAtom s -> emit { op = OpMove; args = [0] }
         | Core.LitInt i -> emit { op = OpMove; args = [i] }
         | Core.LitNil -> emit { op = OpMove; args = [0] }
         | _ -> emit { op = OpMove; args = [0] })
      | Core.CTrue | Core.CFalse ->
        emit { op = OpMove; args = [0] }
      | Core.CNil ->
        emit { op = OpMove; args = [0] }
      | Core.CFun (v, body) ->
        (* Nested function - recurse *)
        gen_expr body
      | Core.CApp (f, args) ->
        (* Function call *)
        List.iter gen_expr args;
        emit { op = OpCall; args = [List.length args] }
      | Core.CLet (v, e1, e2) ->
        gen_expr e1;
        gen_expr e2
      | Core.CSeq (e1, e2) ->
        gen_expr e1;
        gen_expr e2
      | Core.CCase (e, clauses, body) ->
        gen_expr e;
        let lbl = next_label () in
        emit { op = OpJmp; args = [0] };
        List.iter (gen_clause lbl) clauses;
        gen_expr body
      | Core.CReceive (clauses, timeout, body) ->
        (* Receive - complex, use simplified version *)
        emit { op = OpNop; args = [] }
      | Core.CPrimOp (op, args) ->
        List.iter gen_expr args;
        (match op with
         | "+" -> emit { op = OpAdd; args = [] }
         | "-" -> emit { op = OpSub; args = [] }
         | "*" -> emit { op = OpMul; args = [] }
         | "/" -> emit { op = OpDiv; args = [] }
         | _ -> emit { op = OpNop; args = [] })
      | Core.CCons (h, t) ->
        gen_expr h;
        gen_expr t
      | Core.CTuple es ->
        List.iter gen_expr es
      | Core.CNot e ->
        gen_expr e
      | Core.CAndAlso (e1, e2) ->
        gen_expr e1;
        gen_expr e2
      | Core.COrElse (e1, e2) ->
        gen_expr e1;
        gen_expr e2
      | Core.CTry (e, clauses, body, traps) ->
        gen_expr e
      | Core.CThrow e ->
        gen_expr e
      | Core.CExternal (m, f, args) ->
        List.iter gen_expr args;
        emit { op = OpCallExt; args = [List.length args] }
      | Core.CBitString es ->
        List.iter gen_expr es
    and gen_clause exit_label (c: Core.core_clause) =
      let lbl = next_label () in
      (* Pattern matching would go here *)
      gen_expr c.cp_body
    in

    gen_expr body;
    emit { op = OpReturn; args = [] };

    { name; arity; code = List.rev !code; labels }

  (* Generate module from AST *)
  let generate_module (name: string) (exports: string list) (funs: (string * int * Core.expr) list) : module_ =
    {
      name;
      exports;
      functions = List.map (fun (n, a, b) -> generate_function n a b) funs;
    }

  (* Serialize module to BEAM format *)
  let serialize_module (m: module_) : bytes =
    let buf = Buffer.create 1024 in
    (* BEAM magic *)
    Buffer.add_string buf "BEAM";
    (* FOR1 chunk *)
    Buffer.add_string buf "FOR1";
    (* Placeholder for size *)
    let size_pos = Buffer.length buf in
    Buffer.add_bytes buf (Bytes.make 4 '\x00');
    (* AtU8 chunk for atoms *)
    Buffer.add_string buf "AtU8";
    let atoms = m.name :: m.exports in
    let atom_data = Bytes.create (List.length atoms) in
    List.iteri (fun i s -> Bytes.set atom_data i (char_of_int (String.length s))) atoms;
    Buffer.add_bytes buf atom_data;
    (* Code chunk *)
    Buffer.add_string buf "Code";
    let code_data = ref (Bytes.create 256) in
    let code_len = ref 0 in
    List.iter (fun f ->
      let fc = f.code in
      List.iter (fun i ->
        let op_bytes = opcode_to_bytes i.op in
        Array.iter (fun b ->
          if !code_len >= Bytes.length !code_data then
            code_data := Bytes.extend !code_data 0 256;
          Bytes.set !code_data !code_len (char_of_int b);
          code_len := !code_len + 1
        ) op_bytes
      ) fc
    ) m.functions;
    let final_code = Bytes.sub !code_data 0 !code_len in
    Buffer.add_bytes buf final_code;
    Bytes.of_buffer buf

  (* Compile Core expr to bytecode *)
  let compile (name: string) (arity: int) (body: Core.expr) : bytes =
    let mod' = generate_module name [name] [(name, arity, body)] in
    serialize_module mod'
end