//! Tokenizer and parser for ESC source.
//!
//! Tokenization mirrors `FUN_00421150`: operators are matched from a fixed table
//! (longest first), `"…"` is a raw string literal (no escapes), everything else
//! between operators/spaces is a word (identifier, keyword or number). A `.`
//! directly after a digit belongs to a number. Block comments are removed first
//! (`FUN_00421590`); line comments do not exist in the engine.

use crate::ast::*;
use crate::vm::StepMode;

#[derive(Debug, thiserror::Error)]
#[error("line {line}: {msg}")]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Op(&'static str),
}

/// Operator table in matching order (two-character operators first).
const OPS: [&str; 26] = [
    "==", "!=", "++", "--", "<=", ">=", "||", "&&", "=", "+", "-", "{", "}", ";", ",", "(", ")", "<", ">",
    "/", "*", "|", "&", "[", "]", ".",
];

struct Lexed {
    toks: Vec<Tok>,
    lines: Vec<usize>,
}

fn lex(src: &str) -> Result<Lexed, ParseError> {
    // Strip /* */ comments, keeping newlines for line numbers.
    let mut s = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find("/*") {
        s.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => {
                s.extend(rest[i..i + 2 + j + 2].chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
                rest = &rest[i + 2 + j + 2..];
            }
            None => {
                rest = "";
            }
        }
    }
    s.push_str(rest);

    let b: Vec<char> = s.chars().collect();
    let (mut toks, mut lines) = (Vec::new(), Vec::new());
    let mut i = 0;
    let mut line = 1;
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == '\0' {
            i += 1;
            continue;
        }
        if c == '"' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j] != '"' {
                if b[j] == '\n' {
                    line += 1;
                }
                j += 1;
            }
            toks.push(Tok::Str(b[start..j.min(b.len())].iter().collect()));
            lines.push(line);
            i = j + 1;
            continue;
        }
        if let Some(op) = OPS.iter().find(|op| {
            let oc: Vec<char> = op.chars().collect();
            b[i..].starts_with(&oc)
        }) {
            // `.` after a digit is part of a number: handled in the word branch.
            if *op != "." {
                toks.push(Tok::Op(op));
                lines.push(line);
                i += op.len();
                continue;
            }
        }
        // Word: up to whitespace, quote, or an operator character.
        let start = i;
        while i < b.len() {
            let ch = b[i];
            if ch.is_whitespace() || ch == '"' {
                break;
            }
            if ch == '.' {
                if i > start && b[i - 1].is_ascii_digit() {
                    i += 1;
                    continue;
                }
                if i == start {
                    i += 1;
                    continue;
                }
                break;
            }
            if OPS.iter().any(|op| op.starts_with(ch)) {
                break;
            }
            i += 1;
        }
        if i == start {
            i += 1;
            continue;
        }
        toks.push(Tok::Word(b[start..i].iter().collect()));
        lines.push(line);
    }
    Ok(Lexed { toks, lines })
}

struct P {
    toks: Vec<Tok>,
    lines: Vec<usize>,
    pos: usize,
}

type R<T> = Result<T, ParseError>;

impl P {
    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        let line = self.lines.get(self.pos).or(self.lines.last()).copied().unwrap_or(0);
        Err(ParseError { line, msg: msg.into() })
    }
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if self.is_op(op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect_op(&mut self, op: &str) -> R<()> {
        if self.eat_op(op) { Ok(()) } else { self.err(format!("expected '{op}', found {:?}", self.peek())) }
    }
    fn word(&mut self) -> R<String> {
        match self.next() {
            Some(Tok::Word(w)) => Ok(w),
            t => {
                self.pos -= 1;
                self.err(format!("expected identifier, found {t:?}"))
            }
        }
    }
    fn peek_type(&self) -> Option<Type> {
        match self.peek() {
            Some(Tok::Word(w)) => Type::from_keyword(w),
            _ => None,
        }
    }

    fn program(&mut self) -> R<Program> {
        let mut p = Program::default();
        while self.peek().is_some() {
            if let Some(Tok::Word(w)) = self.peek() {
                if w == "#include" {
                    self.pos += 1;
                    match self.next() {
                        Some(Tok::Str(s)) => p.includes.push(s),
                        _ => return self.err("#include expects a string"),
                    }
                    continue;
                }
                if w.starts_with('#') {
                    // Stepping directives at file scope are ignored by the engine.
                    self.pos += 1;
                    continue;
                }
            }
            // The engine's pre-parse (FUN_00422960) only acts on type keywords at
            // brace depth 0; anything else at file scope is skipped.
            let Some(ty) = self.peek_type() else {
                self.pos += 1;
                continue;
            };
            self.pos += 1;
            let Some(Tok::Word(name)) = self.peek().cloned() else { continue };
            self.pos += 1;
            if self.is_op("(") {
                if let Some(f) = self.function(ty, name)? {
                    p.functions.push(f);
                }
            } else {
                p.globals.push(self.decl_rest(ty, name)?);
            }
        }
        Ok(p)
    }

    fn decl_rest(&mut self, ty: Type, name: String) -> R<Stmt> {
        let mut size = None;
        let mut init = None;
        if self.eat_op("[") {
            match self.next() {
                Some(Tok::Word(n)) => size = n.parse().ok(),
                _ => return self.err("array size expected"),
            }
            self.expect_op("]")?;
        } else if self.eat_op("=") {
            let neg = self.eat_op("-");
            match self.next() {
                Some(Tok::Word(n)) => {
                    let v = parse_number(&n).unwrap_or(0);
                    init = Some(if neg { -v } else { v });
                }
                _ => return self.err("initializer must be a number"),
            }
        }
        self.expect_op(";")?;
        Ok(Stmt::Decl { ty, name, size, init })
    }

    /// Function definition; `None` for a prototype (`void f(void);`).
    fn function(&mut self, ret: Type, name: String) -> R<Option<Function>> {
        self.expect_op("(")?;
        let mut params = Vec::new();
        // FUN_00422060: only `type name` pairs are recorded; other tokens (e.g. an
        // untyped `charname`) are skipped.
        while !self.eat_op(")") {
            if self.peek().is_none() {
                return self.err("unterminated parameter list");
            }
            match self.peek_type() {
                Some(Type::Void) => self.pos += 1,
                Some(ty) => {
                    self.pos += 1;
                    if let Some(Tok::Word(n)) = self.peek().cloned() {
                        self.pos += 1;
                        params.push((ty, n));
                    }
                }
                None => self.pos += 1,
            }
        }
        if self.eat_op(";") {
            return Ok(None);
        }
        self.expect_op("{")?;
        let body = self.block_rest()?;
        Ok(Some(Function { name, ret, params, body }))
    }

    /// Skip the rest of a malformed statement (stray `)` etc.) up to `;`.
    fn skip_to_semicolon(&mut self) {
        while let Some(t) = self.peek() {
            match t {
                Tok::Op(";") => {
                    self.pos += 1;
                    return;
                }
                Tok::Op("}") | Tok::Op("{") => return,
                _ => self.pos += 1,
            }
        }
    }

    fn block_rest(&mut self) -> R<Vec<Stmt>> {
        let mut v = Vec::new();
        while !self.eat_op("}") {
            if self.peek().is_none() {
                return self.err("unexpected end of file in block");
            }
            if let Some(s) = self.stmt()? {
                v.push(s);
            }
        }
        Ok(v)
    }

    /// A statement or a braced block, as used for `if`/`while` bodies.
    fn body(&mut self) -> R<Vec<Stmt>> {
        if self.eat_op("{") {
            self.block_rest()
        } else {
            Ok(self.stmt()?.into_iter().collect())
        }
    }

    fn stmt(&mut self) -> R<Option<Stmt>> {
        if self.eat_op(";") {
            return Ok(None);
        }
        if self.eat_op("{") {
            return Ok(Some(Stmt::Block(self.block_rest()?)));
        }
        let Some(Tok::Word(w)) = self.peek().cloned() else {
            // Stray operators / strings at statement level are ignored by the engine.
            self.pos += 1;
            return Ok(None);
        };
        match w.as_str() {
            "#singlestep" => {
                self.pos += 1;
                return Ok(Some(Stmt::Mode(StepMode::Single)));
            }
            "#normalstep" => {
                self.pos += 1;
                return Ok(Some(Stmt::Mode(StepMode::Normal)));
            }
            "#multistep" => {
                self.pos += 1;
                return Ok(Some(Stmt::Mode(StepMode::Multi)));
            }
            "#include" => {
                self.pos += 1;
                return match self.next() {
                    Some(Tok::Str(s)) => Ok(Some(Stmt::Include(s))),
                    _ => self.err("#include expects a string"),
                };
            }
            "if" => {
                self.pos += 1;
                self.expect_op("(")?;
                let cond = self.expr()?;
                self.expect_op(")")?;
                let then = self.body()?;
                let otherwise = if matches!(self.peek(), Some(Tok::Word(w)) if w == "else") {
                    self.pos += 1;
                    Some(self.body()?)
                } else {
                    None
                };
                return Ok(Some(Stmt::If { cond, then, otherwise }));
            }
            "while" => {
                self.pos += 1;
                self.expect_op("(")?;
                let cond = self.expr()?;
                self.expect_op(")")?;
                let body = self.body()?;
                return Ok(Some(Stmt::While { cond, body }));
            }
            "return" => {
                self.pos += 1;
                if self.eat_op(";") {
                    return Ok(Some(Stmt::Return(None)));
                }
                let e = self.expr()?;
                self.eat_op(";");
                return Ok(Some(Stmt::Return(Some(e))));
            }
            _ => {}
        }
        if let Some(ty) = Type::from_keyword(&w) {
            self.pos += 1;
            let name = self.word()?;
            return Ok(Some(self.decl_rest(ty, name)?));
        }
        // Assignment / increment / call.
        let name = self.word()?;
        if self.is_op("(") {
            self.pos += 1;
            let args = self.args()?;
            let e = Expr::Chain(vec![(BinOp::Add, Operand::Call(name, args))]);
            // The engine continues evaluating a statement-level call as an expression.
            let e = self.expr_tail(e)?;
            self.skip_to_semicolon();
            return Ok(Some(Stmt::Expr(e)));
        }
        let index = if self.eat_op("[") {
            let e = self.expr()?;
            self.expect_op("]")?;
            Some(e)
        } else {
            None
        };
        if self.eat_op("=") {
            let value = self.expr()?;
            self.skip_to_semicolon();
            Ok(Some(Stmt::Assign { name, index, value }))
        } else if self.eat_op("++") {
            self.skip_to_semicolon();
            Ok(Some(Stmt::Incr { name, index, delta: 1 }))
        } else if self.eat_op("--") {
            self.skip_to_semicolon();
            Ok(Some(Stmt::Incr { name, index, delta: -1 }))
        } else {
            Ok(Some(Stmt::Unknown(name)))
        }
    }

    fn args(&mut self) -> R<Vec<Expr>> {
        let mut v = Vec::new();
        if self.eat_op(")") {
            return Ok(v);
        }
        loop {
            v.push(self.expr()?);
            if self.eat_op(",") {
                continue;
            }
            // A missing `)` is tolerated: argument evaluation stops at `;`.
            if self.is_op(";") {
                return Ok(v);
            }
            self.expect_op(")")?;
            return Ok(v);
        }
    }

    /// Index of the first top-level `&&`/`||` before the engine's prescan stops
    /// (at `(`, `)`, `;`, `{`), mirroring `FUN_004210f0`. We additionally stop at
    /// `,`: the engine would scan past an argument separator and garble the
    /// argument list, which no shipped script relies on.
    fn prescan_logic(&self) -> Option<usize> {
        let mut k = self.pos;
        while let Some(t) = self.toks.get(k) {
            match t {
                Tok::Op("&&") | Tok::Op("||") => return Some(k),
                Tok::Op("(") | Tok::Op(")") | Tok::Op(";") | Tok::Op("{") | Tok::Op(",") => return None,
                _ => {}
            }
            k += 1;
        }
        None
    }

    fn expr(&mut self) -> R<Expr> {
        if let Some(k) = self.prescan_logic() {
            // Evaluate the left part up to the operator on its own.
            let saved = std::mem::take(&mut self.toks);
            let mut left_toks = saved[..k].to_vec();
            left_toks.push(Tok::Op(";"));
            let mut sub = P { toks: left_toks, lines: self.lines.clone(), pos: self.pos };
            let left = sub.expr_chain()?;
            self.toks = saved;
            self.pos = k;
            let op = if self.eat_op("&&") { LogicOp::And } else { self.next(); LogicOp::Or };
            let right = self.expr()?;
            return Ok(Expr::Logic(Box::new(left), op, Box::new(right)));
        }
        self.expr_chain()
    }

    fn operand(&mut self) -> R<Operand> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(Operand::Str(s)),
            Some(Tok::Op("(")) => {
                let e = self.expr()?;
                self.expect_op(")")?;
                Ok(Operand::Paren(Box::new(e)))
            }
            Some(Tok::Word(w)) => {
                if let Some(n) = parse_number(&w) {
                    return Ok(Operand::Int(n));
                }
                if self.eat_op("(") {
                    return Ok(Operand::Call(w, self.args()?));
                }
                if self.eat_op("[") {
                    let e = self.expr()?;
                    self.expect_op("]")?;
                    return Ok(Operand::Var(w, Some(Box::new(e))));
                }
                Ok(Operand::Var(w, None))
            }
            t => {
                self.pos -= 1;
                self.err(format!("operand expected, found {t:?}"))
            }
        }
    }

    fn binop(&self) -> Option<BinOp> {
        Some(match self.peek()? {
            Tok::Op("+") => BinOp::Add,
            Tok::Op("-") => BinOp::Sub,
            Tok::Op("*") => BinOp::Mul,
            Tok::Op("/") => BinOp::Div,
            Tok::Op("|") => BinOp::BitOr,
            Tok::Op("&") => BinOp::BitAnd,
            Tok::Op("=") => BinOp::Replace,
            _ => return None,
        })
    }

    fn expr_chain(&mut self) -> R<Expr> {
        let mut chain = Vec::new();
        if let Some(op) = self.binop() {
            // Leading operator applies to an implicit 0 (e.g. `-1`).
            self.pos += 1;
            chain.push((BinOp::Add, Operand::Int(0)));
            chain.push((op, self.operand()?));
        } else {
            chain.push((BinOp::Add, self.operand()?));
        }
        while let Some(op) = self.binop() {
            self.pos += 1;
            chain.push((op, self.operand()?));
        }
        self.expr_tail(Expr::Chain(chain))
    }

    /// Comparison or logic operator following a chain: the rest of the expression
    /// becomes the right operand.
    fn expr_tail(&mut self, left: Expr) -> R<Expr> {
        let cmp = match self.peek() {
            Some(Tok::Op("==")) => Some(CmpOp::Eq),
            Some(Tok::Op("!=")) => Some(CmpOp::Ne),
            Some(Tok::Op("<")) => Some(CmpOp::Lt),
            Some(Tok::Op("<=")) => Some(CmpOp::Le),
            Some(Tok::Op(">")) => Some(CmpOp::Gt),
            Some(Tok::Op(">=")) => Some(CmpOp::Ge),
            _ => None,
        };
        if let Some(op) = cmp {
            self.pos += 1;
            let right = self.expr()?;
            return Ok(Expr::Cmp(Box::new(left), op, Box::new(right)));
        }
        let logic = match self.peek() {
            Some(Tok::Op("&&")) => Some(LogicOp::And),
            Some(Tok::Op("||")) => Some(LogicOp::Or),
            _ => None,
        };
        if let Some(op) = logic {
            self.pos += 1;
            let right = self.expr()?;
            return Ok(Expr::Logic(Box::new(left), op, Box::new(right)));
        }
        Ok(left)
    }
}

fn parse_number(w: &str) -> Option<i32> {
    let c = w.chars().next()?;
    if !c.is_ascii_digit() && c != '.' {
        return None;
    }
    if w.contains('.') {
        // atof then truncation to int, as the engine stores float literals.
        let digits: String = w.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        return digits.parse::<f32>().ok().map(|f| f as i32);
    }
    // atoi: leading digits only.
    let digits: String = w.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().map(|v| v as i32)
}

pub fn parse(src: &str) -> Result<Program, ParseError> {
    let Lexed { toks, lines } = lex(src)?;
    P { toks, lines, pos: 0 }.program()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_script() {
        let p = parse(
            "int e[8]; string s; void main(int x, int y)\n{ #multistep e[0]=Create(1,\"a\\b\",2.7); if(x==1 && y!=2) Print(\"x\"); else { i++; } while(i<4){i++;} #normalstep Sleep(-1); }\nvoid exit(void){ KillEmitter(e[0]); }",
        )
        .unwrap();
        assert_eq!(p.functions.len(), 2);
        assert_eq!(p.globals.len(), 2);
        assert_eq!(p.functions[0].params.len(), 2);
    }
}
