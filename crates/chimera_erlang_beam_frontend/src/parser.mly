(* chimera_erlang_beam_frontend - Erlang Parser *)
(* Parser for Erlang syntax producing AST *)

%token Atom Integer Float String Var
%token Plus Minus Multiply Divide Bslash
%token LParen RParen LBracket RBracket LBrace RBrace
%token Pipe Dot Comma Semicolon Colon ColonColon
%token Arrow RArrow Eq EqEq SlashEq Lt Gt LtLt GtGt Lte Gte
%token AndAlso OrElse Not When Fun Case Of End Receive After
%token Begin Try Catch Eof

%start <Ast.expr> expr

%left Pipe
%nonassoc EqEq SlashEq Lt Gt Lte Gte
%left Plus Minus
%left Multiply Divide
%nonassoc AndAlso OrElse
%right Not
%left Apply

%%

expr:
  | a=atom { Ast.Atom a }
  | i=integer { Ast.Integer i }
  | f=float { Ast.Float f }
  | s=string { Ast.String s }
  | v=var { Ast.Var v }
  | LParen e=expr RParen { e }
  | e1=expr Plus e2=expr { Ast.BinOp (e1, Ast.Add, e2) }
  | e1=expr Minus e2=expr { Ast.BinOp (e1, Ast.Sub, e2) }
  | e1=expr Multiply e2=expr { Ast.BinOp (e1, Ast.Mul, e2) }
  | e1=expr Divide e2=expr { Ast.BinOp (e1, Ast.Div, e2) }
  | e1=expr EqEq e2=expr { Ast.BinOp (e1, Ast.Eq, e2) }
  | e1=expr SlashEq e2=expr { Ast.BinOp (e1, Ast.Neq, e2) }
  | e1=expr Lt e2=expr { Ast.BinOp (e1, Ast.Lt, e2) }
  | e1=expr Gt e2=expr { Ast.BinOp (e1, Ast.Gt, e2) }
  | e1=expr Lte e2=expr { Ast.BinOp (e1, Ast.Lte, e2) }
  | e1=expr Gte e2=expr { Ast.BinOp (e1, Ast.Gte, e2) }
  | e1=expr AndAlso e2=expr { Ast.BinOp (e1, Ast.AndAlso, e2) }
  | e1=expr OrElse e2=expr { Ast.BinOp (e1, Ast.OrElse, e2) }
  | Not e=expr { Ast.Not e }
  | Fun v=var Arrow e=expr End { Ast.Fun (v, e) }
  | Fun LParen v=var RParen Arrow e=expr End { Ast.Fun (v, e) }
  | Case e=expr Of c=clause+ Arrow e2=expr End { Ast.Case (e, c, e2) }
  | Receive c=clause+ After t=integer Arrow e=expr End { Ast.Receive (c, t, e) }
  | e1=expr LParen e2=expr RParen { Ast.Call (e1, [e2]) }
  | e1=expr LParen es=separated_list(Comma, expr) RParen { Ast.Call (e1, es) }
  | e1=expr Pipe e2=expr { Ast.BinOp (e1, Ast.Cons, e2) }
  | LBracket es=separated_list(Comma, expr) RBracket { Ast.List es }
  | LBracket RBracket { Ast.List [] }
  | LtLt es=separated_list(Comma, expr) GtGt { Ast.BitString es }
  | LtLt es=separated_list(Comma, bitseg) GtGt { Ast.BitString es }
  | LBrace es=separated_list(Comma, expr) RBrace { Ast.Tuple es }
  | Let v=var Eq e=expr In e2=expr { Ast.Let (v, e, e2) }
  | Begin e=expr End { e }
  | Try e=expr Of c=clause+ Arrow e2=expr Catch traps=list(trap_clause) End
      { Ast.Try (e, c, e2, traps) }
  | Throw e=expr { Ast.Throw e }
  | e1=expr Semicolon e2=expr { Ast.Seq (e1, e2) }

bitseg:
  | e=expr { Ast.BitSeg { expr = e; size = None; opts = [] } }
  | e=expr Colon i=expr { Ast.BitSeg { expr = e; size = Some i; opts = [] } }
  | e=expr Slash opts=bitopts { Ast.BitSeg { expr = e; size = None; opts = opts } }
  | e=expr Colon i=expr Slash opts=bitopts { Ast.BitSeg { expr = e; size = Some i; opts = opts } }

bitopts:
  | o=bitopt { [o] }
  | o=bitopt Slash opts=bitopts { o :: opts }

bitopt:
  | Atom "binary" { Ast.BitBinary }
  | Atom "integer" { Ast.BitInteger }
  | Atom "float" { Ast.BitFloat }
  | Atom "native" { Ast.BitNative }
  | Atom "big" { Ast.BitBig }
  | Atom "little" { Ast.BitLittle }

trap_clause:
  | v=var Arrow e=expr { (v, e) }

clause:
  | p=pattern When g=guard Arrow body=expr { Ast.Clause { pattern = p; guard = g; body } }
  | p=pattern Arrow body=expr { Ast.Clause { pattern = p; guard = Ast.True; body } }

guard:
  | e=expr { e }
  | e=expr Comma gs=separated_list(Comma, guard_test) { Ast.AndAlso (e, gs) }
  | e=expr Semicolon gs=separated_list(Semicolon, guard_test) { Ast.OrElse (e, gs) }

guard_test:
  | e=expr { e }

pattern:
  | a=atom { Ast.PAtom a }
  | i=integer { Ast.PInteger i }
  | p=var { Ast.PVar p }
  | LBracket RBracket { Ast.PList [] }
  | LBracket h=pattern t=pattern_tail RBracket { Ast.PList (h :: t) }
  | LBrace ps=separated_list(Comma, pattern) RBrace { Ast.PTuple ps }
  | p=pattern ColonColon p2=pattern { Ast.PCons (p, p2) }
  | LtLt ps=separated_list(Comma, patseg) GtGt { Ast.PBitString ps }
  | h=atom LParen ps=separated_list(Comma, pattern) RParen { Ast.PTuple (Ast.PAtom h :: ps) }

patseg:
  | p=pattern { Ast.PatSeg { pattern = p; size = None; opts = [] } }
  | p=pattern Colon i=expr { Ast.PatSeg { pattern = p; size = Some i; opts = [] } }

pattern_tail:
  | Pipe p=pattern { p }
  | { Ast.PList [] }

atom:
  | a=Atom { a }
  | Atom ":" a=Atom { a }
  | a=Atom ":" a2=Atom { a }

var:
  | v=Var { v }

integer:
  | i=Integer { i }

float:
  | f=Float { f }

string:
  | s=String { s }

%%