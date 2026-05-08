(* chimera_erlang_beam_frontend - Parser Tests *)

let test_simple_atom () =
  let lexbuf = Lexing.from_string "ok" in
  let tok = Lexer.token lexbuf in
  match tok with
  | Lexer.Atom "ok" -> print_endline "test_simple_atom: PASS"
  | _ -> print_endline "test_simple_atom: FAIL"

let test_integer () =
  let lexbuf = Lexing.from_string "42" in
  let tok = Lexer.token lexbuf in
  match tok with
  | Lexer.Integer 42 -> print_endline "test_integer: PASS"
  | _ -> print_endline "test_integer: FAIL"

let test_addition () =
  let lexbuf = Lexing.from_string "1 + 2" in
  let tok1 = Lexer.token lexbuf in
  let tok2 = Lexer.token lexbuf in
  let tok3 = Lexer.token lexbuf in
  match (tok1, tok2, tok3) with
  | (Lexer.Integer 1, Lexer.Plus, Lexer.Integer 2) -> print_endline "test_addition: PASS"
  | _ -> print_endline "test_addition: FAIL"

let test_case_expr () =
  let input = "case X of a -> b; c -> d end" in
  let lexbuf = Lexing.from_string input in
  let rec count_tokens n =
    let tok = Lexer.token lexbuf in
    match tok with
    | Lexer.Eof -> n
    | _ -> count_tokens (n + 1)
  in
  let count = count_tokens 0 in
  if count > 0 then print_endline "test_case_expr: PASS"
  else print_endline "test_case_expr: FAIL"

let test_receive () =
  let input = "receive after 5000 -> ok end" in
  let lexbuf = Lexing.from_string input in
  let rec count_tokens n =
    let tok = Lexer.token lexbuf in
    match tok with
    | Lexer.Eof -> n
    | _ -> count_tokens (n + 1)
  in
  let count = count_tokens 0 in
  if count > 0 then print_endline "test_receive: PASS"
  else print_endline "test_receive: FAIL"

let test_bitstring () =
  let input = "<<1,2,3>>" in
  let lexbuf = Lexing.from_string input in
  let rec count_tokens n =
    let tok = Lexer.token lexbuf in
    match tok with
    | Lexer.Eof -> n
    | _ -> count_tokens (n + 1)
  in
  let count = count_tokens 0 in
  if count > 0 then print_endline "test_bitstring: PASS"
  else print_endline "test_bitstring: FAIL"

let run_tests () =
  print_endline "Running OCaml Frontend Tests...";
  test_simple_atom ();
  test_integer ();
  test_addition ();
  test_case_expr ();
  test_receive ();
  test_bitstring ();
  print_endline "All tests completed."

let () = run_tests ()