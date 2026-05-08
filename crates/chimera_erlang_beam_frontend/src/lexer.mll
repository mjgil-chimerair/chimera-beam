(* chimera_erlang_beam_frontend - Erlang Lexer *)
(* Tokenizer for Erlang syntax *)

{
  type token =
    | Atom of string
    | Integer of int
    | Float of float
    | String of string
    | Var of string
    | Plus
    | Minus
    | Multiply
    | Divide
    | LParen
    | RParen
    | LBracket
    | RBracket
    | LBrace
    | RBrace
    | Pipe
    | Dot
    | Comma
    | Semicolon
    | Colon
    | Arrow
    | Eq
    | EqEq
    | SlashEq
    | Lt
    | Gt
    | Lte
    | Gte
    | AndAlso
    | OrElse
    | Not
    | When
    | Fun
    | Case
    | Of
    | End
    | Receive
    | After
    | Begin
    | Try
    | Catch
    | Bslash
    | LtLt
    | GtGt
    | RArrow
    | ColonColon
    | Eof

  let keywords = [
    ("andalso", AndAlso);
    ("orelse", OrElse);
    ("not", Not);
    ("when", When);
    ("fun", Fun);
    ("case", Case);
    ("of", Of);
    ("end", End);
    ("receive", Receive);
    ("after", After);
    ("begin", Begin);
    ("try", Try);
    ("catch", Catch)
  ]

  let lookup_keyword s =
    try List.assoc s keywords
    with Not_found -> Atom s
}

let whitespace = ['\x20' '\x09' '\x0a' '\x0d']
let lowercase = ['a'-'z']
let uppercase = ['A'-'Z']
let digit = ['0'-'9']
let letter = lowercase | uppercase | '_'

rule token = parse
  | whitespace+ { token lexbuf }
  | "%" [^'\n']* { token lexbuf }
  | "%!" [^'\n']* { token lexbuf }
  | lowercase (letter | digit)* as s { lookup_keyword s }
  | uppercase (letter | digit | '_')* as s { Var s }
  | digit+ as s { Integer (int_of_string s) }
  | digit+ '.' digit+ (['e' 'E'] ['+' '-']? digit+)? as s { Float (float_of_string s) }
  | '"' ([^'"' '\\'] | '\\' ['n' 't' 'r' '"' '\\'])* '"' as s {
      let str = String.sub s 1 (String.length s - 2) in
      String (unescape str) }
  | '<<' { LtLt }
  | '>>' { GtGt }
  | "->" { Arrow }
  | "=>" { RArrow }
  | "=:=" { EqEq }
  | "=/=" { SlashEq }
  | "=<" { Lte }
  | ">=" { Gte }
  | "::" { ColonColon }
  | '+' { Plus }
  | '-' { Minus }
  | '*' { Multiply }
  | '/' { Divide }
  | '\\' { Bslash }
  | '(' { LParen }
  | ')' { RParen }
  | '[' { LBracket }
  | ']' { RBracket }
  | '{' { LBrace }
  | '}' { RBrace }
  | '|' { Pipe }
  | '.' { Dot }
  | ',' { Comma }
  | ';' { Semicolon }
  | ':' { Colon }
  | '=' { Eq }
  | '<' { Lt }
  | '>' { Gt }
  | eof { Eof }

and unescape s =
  let rec go i acc =
    if i >= String.length s then List.rev acc else
    match s.[i] with
    | '\\' when i + 1 < String.length s ->
      let c = match s.[i+1] with
       | 'n' -> '\n'
       | 't' -> '\t'
       | 'r' -> '\r'
       | '"' -> '"'
       | '\\' -> '\\'
       | _ -> s.[i+1] in
      go (i + 2) (c :: acc)
    | c -> go (i + 1) (c :: acc)
  in
  let chars = go 0 [] in
  String.of_list chars