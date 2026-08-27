use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    env, fmt, fs,
    net::{TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    rc::Rc,
};

#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

mod checker;
mod diagnostics;
mod formatter;
mod native;
mod profiler;
mod project;
mod reference;
mod test_runner;
mod extensions {
    include!(concat!(env!("OUT_DIR"), "/isen_extensions.rs"));
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
struct Error {
    line: usize,
    message: String,
    clean_exit: bool,
    source: Option<PathBuf>,
    problem: Option<Box<Value>>,
}
impl Error {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            clean_exit: false,
            source: None,
            problem: None,
        }
    }
    fn clean_exit(line: usize) -> Self {
        Self {
            line,
            message: "exit".into(),
            clean_exit: true,
            source: None,
            problem: None,
        }
    }
    fn with_source(mut self, source: &Path) -> Self {
        if self.source.is_none() {
            self.source = Some(source.to_owned());
        }
        self
    }
}
impl fmt::Debug for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Error")
            .field("line", &self.line)
            .field("message", &self.message)
            .field("clean_exit", &self.clean_exit)
            .field("source", &self.source)
            .field(
                "problem_type",
                &self.problem.as_ref().map(|value| &value.ty),
            )
            .finish()
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.source {
            write!(f, "{}:{}: {}", source.display(), self.line, self.message)
        } else {
            write!(f, "{}: {}", self.line, self.message)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    Int,
    Float,
    Bool,
    String,
    Json,
    Naught,
    Perchance(Box<Ty>),
    List(Box<Ty>),
    Arr(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    UdpSocket,
    UdpPacket,
    TcpListener,
    TcpStream,
    HttpResponse,
    Named(String),
    Unit,
}
impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::Bool => write!(f, "bool"),
            Ty::String => write!(f, "string"),
            Ty::Json => write!(f, "json"),
            Ty::Naught => write!(f, "naught"),
            Ty::Perchance(t) => write!(f, "perchance[{t}]"),
            Ty::List(t) => write!(f, "list[{t}]"),
            Ty::Arr(t) => write!(f, "arr[{t}]"),
            Ty::Map(k, v) => write!(f, "map[{k}, {v}]"),
            Ty::UdpSocket => write!(f, "udp_socket"),
            Ty::UdpPacket => write!(f, "udp_packet"),
            Ty::TcpListener => write!(f, "tcp_listener"),
            Ty::TcpStream => write!(f, "tcp_stream"),
            Ty::HttpResponse => write!(f, "http_response"),
            Ty::Named(n) => write!(f, "{n}"),
            Ty::Unit => write!(f, "unit"),
        }
    }
}

const SOURCE_LEAF_TYPES: &[(&str, Ty)] = &[
    ("int", Ty::Int),
    ("int64", Ty::Int),
    ("float", Ty::Float),
    ("bool", Ty::Bool),
    ("string", Ty::String),
    ("json", Ty::Json),
    ("naught", Ty::Naught),
    ("unit", Ty::Unit),
    ("udp_socket", Ty::UdpSocket),
    ("udp_packet", Ty::UdpPacket),
    ("tcp_listener", Ty::TcpListener),
    ("tcp_stream", Ty::TcpStream),
    ("http_response", Ty::HttpResponse),
];

fn source_leaf_type(name: &str) -> Option<Ty> {
    SOURCE_LEAF_TYPES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, ty)| ty.clone())
}

struct BinaryOperatorGroup {
    precedence: u8,
    operators: &'static [&'static str],
    operands: &'static str,
}

const BINARY_OPERATOR_GROUPS: &[BinaryOperatorGroup] = &[
    BinaryOperatorGroup {
        precedence: 1,
        operators: &["||"],
        operands: "bool, bool",
    },
    BinaryOperatorGroup {
        precedence: 2,
        operators: &["&&"],
        operands: "bool, bool",
    },
    BinaryOperatorGroup {
        precedence: 3,
        operators: &["==", "!="],
        operands: "compatible equality-supporting values",
    },
    BinaryOperatorGroup {
        precedence: 4,
        operators: &["<", "<=", ">", ">="],
        operands: "matching int, float, or string",
    },
    BinaryOperatorGroup {
        precedence: 5,
        operators: &["<<", ">>"],
        operands: "int, int; shift count 0 through 63",
    },
    BinaryOperatorGroup {
        precedence: 6,
        operators: &["+", "-", "|", "^"],
        operands: "matching arithmetic values; bitwise operators require int",
    },
    BinaryOperatorGroup {
        precedence: 7,
        operators: &["*", "/", "%", "&"],
        operands: "matching arithmetic values; bitwise operators require int",
    },
];

fn binary_precedence(operator: &str) -> u8 {
    BINARY_OPERATOR_GROUPS
        .iter()
        .find(|group| group.operators.contains(&operator))
        .map_or(0, |group| group.precedence)
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Ident(String),
    Int(i64),
    Float(f64),
    String(String),
    Symbol(String),
    Eof,
}
#[derive(Clone, Debug)]
struct Spanned {
    token: Token,
    line: usize,
}

fn lex(src: &str) -> Result<Vec<Spanned>> {
    let mut out = vec![];
    let mut it = src.chars().peekable();
    let mut line = 1;
    while let Some(c) = it.next() {
        match c {
            ' ' | '\t' | '\r' => {}
            '\n' => line += 1,
            '#' => {
                if it.peek() == Some(&'{') {
                    it.next();
                    out.push(Spanned {
                        token: Token::Symbol("#{".into()),
                        line,
                    });
                } else {
                    for x in it.by_ref() {
                        if x == '\n' {
                            line += 1;
                            break;
                        }
                    }
                }
            }
            '@' => {
                if it.peek() == Some(&'@') {
                    it.next();
                    out.push(Spanned {
                        token: Token::Symbol("@@".into()),
                        line,
                    });
                } else if it.peek() == Some(&'[') {
                    it.next();
                    out.push(Spanned {
                        token: Token::Symbol("@[".into()),
                        line,
                    });
                } else {
                    return Err(Error::new(line, "expected '@' or '[' after '@'"));
                }
            }
            '/' => {
                if it.peek() == Some(&'/') {
                    it.next();
                    for x in it.by_ref() {
                        if x == '\n' {
                            line += 1;
                            break;
                        }
                    }
                } else if it.peek() == Some(&'=') {
                    it.next();
                    out.push(Spanned {
                        token: Token::Symbol("/=".into()),
                        line,
                    });
                } else {
                    out.push(Spanned {
                        token: Token::Symbol("/".into()),
                        line,
                    });
                }
            }
            '"' => {
                let mut s = String::new();
                loop {
                    match it.next() {
                        Some('"') => break,
                        Some('\\') => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('"') => s.push('"'),
                            Some('\\') => s.push('\\'),
                            Some(x) => {
                                return Err(Error::new(line, format!("unknown escape \\{x}")));
                            }
                            None => return Err(Error::new(line, "unterminated string")),
                        },
                        Some('\n') => return Err(Error::new(line, "unterminated string")),
                        Some(x) => s.push(x),
                        None => return Err(Error::new(line, "unterminated string")),
                    }
                }
                out.push(Spanned {
                    token: Token::String(s),
                    line,
                });
            }
            '0'..='9' => {
                let mut s = c.to_string();
                while matches!(it.peek(), Some('0'..='9')) {
                    s.push(it.next().unwrap());
                }
                let mut lookahead = it.clone();
                if lookahead.next() == Some('.') && matches!(lookahead.next(), Some('0'..='9')) {
                    s.push(it.next().unwrap());
                    while matches!(it.peek(), Some('0'..='9')) {
                        s.push(it.next().unwrap())
                    }
                    let n = s.parse().map_err(|_| Error::new(line, "invalid float"))?;
                    out.push(Spanned {
                        token: Token::Float(n),
                        line,
                    });
                } else {
                    let n = s
                        .parse()
                        .map_err(|_| Error::new(line, "integer is too large"))?;
                    out.push(Spanned {
                        token: Token::Int(n),
                        line,
                    });
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut s = c.to_string();
                while matches!(it.peek(), Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_')) {
                    s.push(it.next().unwrap());
                }
                out.push(Spanned {
                    token: Token::Ident(s),
                    line,
                });
            }
            '\\' => {
                if it.next() == Some('$') {
                    out.push(Spanned {
                        token: Token::Symbol("\\$".into()),
                        line,
                    });
                } else {
                    return Err(Error::new(line, "expected '$' after '\\'"));
                }
            }
            '=' | '!' | '<' | '>' | '&' | '|' | '+' | '-' | '*' | '%' | '^' => {
                let mut s = c.to_string();
                if matches!(
                    (c, it.peek()),
                    ('=', Some('='))
                        | ('!', Some('='))
                        | ('<', Some('='))
                        | ('>', Some('='))
                        | ('&', Some('&'))
                        | ('|', Some('|'))
                        | ('+', Some('='))
                        | ('-', Some('='))
                        | ('*', Some('='))
                        | ('/', Some('='))
                        | ('%', Some('='))
                        | ('&', Some('='))
                        | ('|', Some('='))
                        | ('^', Some('='))
                        | ('<', Some('<'))
                        | ('>', Some('>'))
                ) {
                    s.push(it.next().unwrap());
                    if matches!(s.as_str(), "<<" | ">>") && it.peek() == Some(&'=') {
                        s.push(it.next().unwrap());
                    }
                };
                out.push(Spanned {
                    token: Token::Symbol(s),
                    line,
                });
            }
            '~' | '(' | ')' | '{' | '}' | '[' | ']' | ',' | ':' | ';' | '.' | '$' => {
                out.push(Spanned {
                    token: Token::Symbol(c.to_string()),
                    line,
                })
            }
            _ => return Err(Error::new(line, format!("unexpected character {c:?}"))),
        }
    }
    out.push(Spanned {
        token: Token::Eof,
        line,
    });
    Ok(out)
}

#[derive(Clone)]
enum Expr {
    Int(i64, usize),
    Float(f64, usize),
    Bool(bool, usize),
    String(String, usize),
    Naught(usize),
    Name(String, usize),
    Declare(String, Option<Ty>, Box<Expr>, usize),
    List(Vec<Expr>, usize),
    Arr(Vec<Expr>, usize),
    Map(Vec<(Expr, Expr)>, usize),
    Form(String, Vec<(String, Expr)>, usize),
    Unary(String, Box<Expr>, usize),
    Binary(Box<Expr>, String, Box<Expr>, usize),
    Call(Box<Expr>, Vec<Expr>, usize),
    Cast(Box<Expr>, Ty, usize),
    Index(Box<Expr>, Box<Expr>, usize),
    Field(Box<Expr>, String, usize),
}
impl Expr {
    fn line(&self) -> usize {
        match self {
            Expr::Int(_, l)
            | Expr::Float(_, l)
            | Expr::Bool(_, l)
            | Expr::String(_, l)
            | Expr::Naught(l)
            | Expr::Name(_, l)
            | Expr::Declare(_, _, _, l)
            | Expr::List(_, l)
            | Expr::Arr(_, l)
            | Expr::Map(_, l)
            | Expr::Form(_, _, l)
            | Expr::Unary(_, _, l)
            | Expr::Binary(_, _, _, l)
            | Expr::Call(_, _, l)
            | Expr::Cast(_, _, l)
            | Expr::Index(_, _, l)
            | Expr::Field(_, _, l) => *l,
        }
    }
}
#[allow(dead_code)]
#[derive(Clone)]
enum Stmt {
    Let(String, Option<Ty>, Expr, usize),
    Assign(Expr, Option<String>, Expr, usize),
    Show(Vec<Expr>, usize),
    Warn(Vec<Expr>, usize),
    Raise(Vec<Expr>, usize),
    QuietRaise(Vec<Expr>, usize),
    Attempt(
        Vec<Stmt>,
        Vec<(String, Ty, Vec<Stmt>)>,
        Option<Vec<Stmt>>,
        usize,
    ),
    If(Expr, Vec<Stmt>, Vec<Stmt>, usize),
    While(Expr, Vec<Stmt>, usize),
    For(String, Expr, Vec<Stmt>, usize),
    Fn(String, Vec<String>, Vec<(String, Ty)>, Ty, Vec<Stmt>, usize),
    Return(Expr, usize),
    Enough(usize),
    Onwards(usize),
    Exit(usize),
    Borrow(String, Option<String>, Option<String>, usize),
    Share(String, usize),
    Namespace(String, Vec<Stmt>, usize),
    Form(String, Vec<(String, Ty)>, usize),
    Problem(String, Vec<(String, Ty)>, usize),
    Expr(Expr),
}

struct Parser {
    toks: Vec<Spanned>,
    at: usize,
}
fn is_language_word(name: &str) -> bool {
    matches!(
        name,
        "borrow"
            | "from"
            | "as"
            | "share"
            | "dec"
            | "say"
            | "shout"
            | "scream"
            | "raise"
            | "attempt"
            | "recover"
            | "always"
            | "if"
            | "else"
            | "aslongas"
            | "each"
            | "in"
            | "given"
            | "ret"
            | "enough"
            | "onwards"
            | "exit"
            | "space"
            | "form"
            | "problem"
            | "true"
            | "false"
            | "naught"
            | "unit"
            | "int"
            | "int64"
            | "float"
            | "bool"
            | "string"
            | "json"
            | "udp_socket"
            | "udp_packet"
            | "tcp_listener"
            | "tcp_stream"
            | "http_response"
            | "perchance"
            | "list"
            | "arr"
            | "map"
    )
}
impl Parser {
    fn new(toks: Vec<Spanned>) -> Self {
        Self { toks, at: 0 }
    }
    fn cur(&self) -> &Spanned {
        &self.toks[self.at]
    }
    fn line(&self) -> usize {
        self.cur().line
    }
    fn sym(&self, s: &str) -> bool {
        matches!(&self.cur().token,Token::Symbol(x) if x==s)
    }
    fn word(&self, s: &str) -> bool {
        matches!(&self.cur().token,Token::Ident(x) if x==s)
    }
    fn take(&mut self) {
        if !matches!(self.cur().token, Token::Eof) {
            self.at += 1
        }
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        if self.sym(s) {
            self.take();
            true
        } else {
            false
        }
    }
    fn need_sym(&mut self, s: &str) -> Result<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(Error::new(self.line(), format!("expected '{s}'")))
        }
    }
    fn ident(&mut self) -> Result<String> {
        if let Token::Ident(s) = self.cur().token.clone() {
            self.take();
            Ok(s)
        } else {
            Err(Error::new(self.line(), "expected name"))
        }
    }
    fn program(&mut self) -> Result<Vec<Stmt>> {
        let mut v = vec![];
        while !matches!(self.cur().token, Token::Eof) {
            if self.eat_sym(";") {
                continue;
            }
            v.push(self.stmt()?)
        }
        Ok(v)
    }
    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.need_sym("$")?;
        let mut v = vec![];
        while !self.sym("\\$") {
            if matches!(self.cur().token, Token::Eof) {
                return Err(Error::new(self.line(), "expected '\\$'"));
            }
            if self.eat_sym(";") {
                continue;
            }
            v.push(self.stmt()?)
        }
        self.take();
        Ok(v)
    }
    fn stmt(&mut self) -> Result<Stmt> {
        let l = self.line();
        if self.word("borrow") {
            self.take();
            let name = self.ident()?;
            let source = if self.word("from") {
                self.take();
                let Token::String(path) = self.cur().token.clone() else {
                    return Err(Error::new(self.line(), "expected a quoted package path"));
                };
                self.take();
                Some(path)
            } else {
                None
            };
            let alias = if self.word("as") {
                if source.is_none() {
                    return Err(Error::new(
                        self.line(),
                        "only names borrowed from a stash may be aliased",
                    ));
                }
                self.take();
                let alias = self.ident()?;
                if is_language_word(&alias) {
                    return Err(Error::new(
                        l,
                        format!("'{alias}' cannot be used as a borrow alias"),
                    ));
                }
                Some(alias)
            } else {
                None
            };
            return Ok(Stmt::Borrow(name, source, alias, l));
        }
        if self.word("share") {
            self.take();
            return Ok(Stmt::Share(self.ident()?, l));
        }
        if self.word("dec") {
            self.take();
            let n = self.ident()?;
            let t = if self.eat_sym("@@") {
                Some(self.ty()?)
            } else {
                None
            };
            self.need_sym("=")?;
            return Ok(Stmt::Let(n, t, self.expr(1)?, l));
        }
        if self.word("say") {
            self.take();
            self.need_sym("(")?;
            let mut a = vec![];
            if !self.sym(")") {
                loop {
                    a.push(self.expr(1)?);
                    if !self.eat_sym(",") || self.sym(")") {
                        break;
                    }
                }
            }
            self.need_sym(")")?;
            return Ok(Stmt::Show(a, l));
        }
        if self.word("shout") {
            self.take();
            self.need_sym("(")?;
            let mut arguments = vec![];
            if !self.sym(")") {
                loop {
                    arguments.push(self.expr(1)?);
                    if !self.eat_sym(",") || self.sym(")") {
                        break;
                    }
                }
            }
            self.need_sym(")")?;
            return Ok(Stmt::Warn(arguments, l));
        }
        if self.word("scream") {
            self.take();
            self.need_sym("(")?;
            let mut arguments = vec![];
            if !self.sym(")") {
                loop {
                    arguments.push(self.expr(1)?);
                    if !self.eat_sym(",") || self.sym(")") {
                        break;
                    }
                }
            }
            self.need_sym(")")?;
            return Ok(Stmt::Raise(arguments, l));
        }
        if self.word("raise") {
            self.take();
            self.need_sym("(")?;
            let mut arguments = vec![];
            if !self.sym(")") {
                loop {
                    arguments.push(self.expr(1)?);
                    if !self.eat_sym(",") || self.sym(")") {
                        break;
                    }
                }
            }
            self.need_sym(")")?;
            return Ok(Stmt::QuietRaise(arguments, l));
        }
        if self.word("attempt") {
            self.take();
            let body = self.block()?;
            let mut recoveries = Vec::new();
            while self.word("recover") {
                self.take();
                let problem = self.ident()?;
                self.need_sym("@@")?;
                let problem_type = self.ty()?;
                recoveries.push((problem, problem_type, self.block()?));
            }
            let always = if self.word("always") {
                self.take();
                Some(self.block()?)
            } else {
                None
            };
            if recoveries.is_empty() && always.is_none() {
                return Err(Error::new(
                    l,
                    "attempt requires a recover block, an always block, or both",
                ));
            }
            return Ok(Stmt::Attempt(body, recoveries, always, l));
        }
        if self.word("if") {
            self.take();
            let c = self.expr(1)?;
            let yes = self.block()?;
            let no = if self.word("else") {
                self.take();
                if self.word("if") {
                    vec![self.stmt()?]
                } else {
                    self.block()?
                }
            } else {
                vec![]
            };
            return Ok(Stmt::If(c, yes, no, l));
        }
        if self.word("aslongas") {
            self.take();
            let c = self.expr(1)?;
            return Ok(Stmt::While(c, self.block()?, l));
        }
        if self.word("each") {
            self.take();
            let n = self.ident()?;
            if !self.word("in") {
                return Err(Error::new(self.line(), "expected 'in' after an each name"));
            }
            self.take();
            let e = self.expr(1)?;
            return Ok(Stmt::For(n, e, self.block()?, l));
        }
        if self.word("given") {
            self.take();
            let n = self.ident()?;
            let mut generics = vec![];
            if self.eat_sym("[") {
                if self.sym("]") {
                    return Err(Error::new(
                        self.line(),
                        "a generic parameter list cannot be empty",
                    ));
                }
                loop {
                    generics.push(self.ident()?);
                    if !self.eat_sym(",") || self.sym("]") {
                        break;
                    }
                }
                self.need_sym("]")?;
            }
            self.need_sym("(")?;
            let mut p = vec![];
            if !self.sym(")") {
                loop {
                    let pn = self.ident()?;
                    self.need_sym("@@")?;
                    p.push((pn, self.ty()?));
                    if !self.eat_sym(",") || self.sym(")") {
                        break;
                    }
                }
            }
            self.need_sym(")")?;
            self.need_sym("@@")?;
            let r = self.ty()?;
            return Ok(Stmt::Fn(n, generics, p, r, self.block()?, l));
        }
        if self.word("ret") {
            self.take();
            return Ok(Stmt::Return(self.expr(1)?, l));
        }
        if self.word("enough") {
            self.take();
            return Ok(Stmt::Enough(l));
        }
        if self.word("onwards") {
            self.take();
            return Ok(Stmt::Onwards(l));
        }
        if self.word("exit") {
            self.take();
            return Ok(Stmt::Exit(l));
        }
        if self.word("space") {
            self.take();
            return Ok(Stmt::Namespace(self.ident()?, self.block()?, l));
        }
        if self.word("form") {
            self.take();
            let n = self.ident()?;
            self.need_sym("$")?;
            let mut fs = vec![];
            while !self.sym("\\$") {
                let f = self.ident()?;
                self.need_sym("@@")?;
                fs.push((f, self.ty()?));
                self.eat_sym(",");
                self.eat_sym(";");
            }
            self.take();
            return Ok(Stmt::Form(n, fs, l));
        }
        if self.word("problem") {
            self.take();
            let n = self.ident()?;
            self.need_sym("$")?;
            let mut fs = vec![];
            while !self.sym("\\$") {
                let f = self.ident()?;
                self.need_sym("@@")?;
                fs.push((f, self.ty()?));
                self.eat_sym(",");
                self.eat_sym(";");
            }
            self.take();
            if fs.iter().any(|(name, _)| name == "message") {
                return Err(Error::new(
                    l,
                    "problem declarations inherit the message field",
                ));
            }
            fs.insert(0, ("message".into(), Ty::String));
            return Ok(Stmt::Problem(n, fs, l));
        }
        let target = self.expr(1)?;
        let assignment = match &self.cur().token {
            Token::Symbol(symbol)
                if matches!(
                    symbol.as_str(),
                    "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
                ) =>
            {
                Some(symbol.clone())
            }
            _ => None,
        };
        if let Some(operator) = assignment {
            self.take();
            let val = self.expr(1)?;
            let compound = operator
                .strip_suffix('=')
                .filter(|operator| !operator.is_empty())
                .map(str::to_owned);
            Ok(Stmt::Assign(target, compound, val, l))
        } else {
            Ok(Stmt::Expr(target))
        }
    }
    fn ty(&mut self) -> Result<Ty> {
        let n = self.ident()?;
        if let Some(ty) = source_leaf_type(&n) {
            return Ok(ty);
        }
        match n.as_str() {
            "perchance" => {
                self.need_sym("[")?;
                let inner = self.ty()?;
                self.need_sym("]")?;
                if matches!(inner, Ty::Naught | Ty::Perchance(_)) {
                    return Err(Error::new(
                        self.line(),
                        "perchance must contain one non-naught type",
                    ));
                }
                Ok(Ty::Perchance(Box::new(inner)))
            }
            "list" => {
                self.need_sym("[")?;
                let x = self.ty()?;
                self.need_sym("]")?;
                Ok(Ty::List(Box::new(x)))
            }
            "arr" => {
                self.need_sym("[")?;
                let x = self.ty()?;
                self.need_sym("]")?;
                Ok(Ty::Arr(Box::new(x)))
            }
            "map" => {
                self.need_sym("[")?;
                let k = self.ty()?;
                self.need_sym(",")?;
                let v = self.ty()?;
                self.need_sym("]")?;
                Ok(Ty::Map(Box::new(k), Box::new(v)))
            }
            _ => Ok(Ty::Named(n)),
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match &self.cur().token {
                Token::Symbol(s) => s.clone(),
                _ => String::new(),
            };
            let p = binary_precedence(&op);
            if p < min {
                break;
            }
            let l = self.line();
            self.take();
            let right = self.expr(p + 1)?;
            left = Expr::Binary(Box::new(left), op, Box::new(right), l)
        }
        Ok(left)
    }
    fn unary(&mut self) -> Result<Expr> {
        let l = self.line();
        if self.sym("-") || self.sym("!") || self.sym("~") {
            let o = match &self.cur().token {
                Token::Symbol(x) => x.clone(),
                _ => unreachable!(),
            };
            self.take();
            return Ok(Expr::Unary(o, Box::new(self.unary()?), l));
        }
        self.postfix()
    }
    fn postfix(&mut self) -> Result<Expr> {
        let mut e = self.primary()?;
        loop {
            let l = self.line();
            if self.eat_sym("(") {
                let mut a = vec![];
                if !self.sym(")") {
                    loop {
                        a.push(self.expr(1)?);
                        if !self.eat_sym(",") || self.sym(")") {
                            break;
                        }
                    }
                }
                self.need_sym(")")?;
                e = Expr::Call(Box::new(e), a, l)
            } else if self.eat_sym("[") {
                let x = self.expr(1)?;
                self.need_sym("]")?;
                e = Expr::Index(Box::new(e), Box::new(x), l)
            } else if self.eat_sym(".") {
                let name = self.ident()?;
                if name == "pour_into" {
                    self.need_sym("(")?;
                    let ty = self.ty()?;
                    self.need_sym(")")?;
                    e = Expr::Cast(Box::new(e), ty, l)
                } else {
                    e = Expr::Field(Box::new(e), name, l)
                }
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn primary(&mut self) -> Result<Expr> {
        let l = self.line();
        match self.cur().token.clone() {
            Token::Int(n) => {
                self.take();
                Ok(Expr::Int(n, l))
            }
            Token::Float(n) => {
                self.take();
                Ok(Expr::Float(n, l))
            }
            Token::String(s) => {
                self.take();
                Ok(Expr::String(s, l))
            }
            Token::Ident(s) if s == "true" || s == "false" => {
                self.take();
                Ok(Expr::Bool(s == "true", l))
            }
            Token::Ident(s) if s == "naught" => {
                self.take();
                Ok(Expr::Naught(l))
            }
            Token::Ident(s) if s == "dec" => {
                self.take();
                let name = self.ident()?;
                let ty = if self.eat_sym("@@") {
                    Some(self.ty()?)
                } else {
                    None
                };
                self.need_sym("=")?;
                let value = self.expr(1)?;
                Ok(Expr::Declare(name, ty, Box::new(value), l))
            }
            Token::Ident(s) => {
                self.take();
                if self.sym("$") && s.chars().next().is_some_and(char::is_uppercase) {
                    let fs = self.fields()?;
                    Ok(Expr::Form(s, fs, l))
                } else {
                    Ok(Expr::Name(s, l))
                }
            }
            Token::Symbol(s) if s == "[" => {
                self.take();
                let mut xs = vec![];
                if !self.sym("]") {
                    loop {
                        xs.push(self.expr(1)?);
                        if !self.eat_sym(",") || self.sym("]") {
                            break;
                        }
                    }
                }
                self.need_sym("]")?;
                Ok(Expr::List(xs, l))
            }
            Token::Symbol(s) if s == "@[" => {
                self.take();
                let mut xs = vec![];
                if !self.sym("]") {
                    loop {
                        xs.push(self.expr(1)?);
                        if !self.eat_sym(",") || self.sym("]") {
                            break;
                        }
                    }
                }
                self.need_sym("]")?;
                Ok(Expr::Arr(xs, l))
            }
            Token::Symbol(s) if s == "#{" => {
                self.take();
                let mut xs = vec![];
                if !self.sym("}") {
                    loop {
                        let k = self.expr(1)?;
                        self.need_sym(":")?;
                        let v = self.expr(1)?;
                        xs.push((k, v));
                        if !self.eat_sym(",") || self.sym("}") {
                            break;
                        }
                    }
                }
                self.need_sym("}")?;
                Ok(Expr::Map(xs, l))
            }
            Token::Symbol(s) if s == "(" => {
                self.take();
                let e = self.expr(1)?;
                self.need_sym(")")?;
                Ok(e)
            }
            _ => Err(Error::new(l, "expected an expression")),
        }
    }
    fn fields(&mut self) -> Result<Vec<(String, Expr)>> {
        self.need_sym("$")?;
        let mut fs = vec![];
        while !self.sym("\\$") {
            let n = self.ident()?;
            self.need_sym(":")?;
            fs.push((n, self.expr(1)?));
            if !self.eat_sym(",") || self.sym("\\$") {
                break;
            }
        }
        self.need_sym("\\$")?;
        Ok(fs)
    }
}

#[derive(Clone)]
struct Value {
    ty: Ty,
    data: Data,
}
#[allow(dead_code)]
#[derive(Clone)]
enum Data {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Json(serde_json::Value),
    Naught,
    List(Rc<RefCell<Vec<Value>>>),
    Arr(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<BTreeMap<String, Value>>>),
    UdpSocket(Rc<UdpSocket>),
    UdpPacket {
        host: String,
        port: i64,
        bytes: Vec<u8>,
    },
    TcpListener(Rc<TcpListener>),
    TcpStream(Rc<RefCell<TcpStream>>),
    HttpResponse {
        status: i64,
        reason: String,
        version: String,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    },
    Form(String, Rc<RefCell<BTreeMap<String, Value>>>),
    Problem(String, Rc<RefCell<BTreeMap<String, Value>>>),
    Function(Function),
    Builtin(Builtin),
    Namespace(EnvRef),
    Unit,
}
#[derive(Clone)]
struct Function {
    name: String,
    line: usize,
    params: Vec<(String, Ty)>,
    ret: Ty,
    body: Vec<Stmt>,
    closure: EnvRef,
    source: Option<PathBuf>,
    generics: HashSet<String>,
    generic_bindings: HashMap<String, Ty>,
}
#[derive(Clone)]
enum Builtin {
    Native {
        space: &'static str,
        name: &'static str,
        call: native::NativeCallback,
    },
    NativeRuntime {
        space: &'static str,
        name: &'static str,
        call: native::NativeRuntimeCallback,
    },
    Size,
}
type EnvRef = Rc<RefCell<Env>>;
#[derive(Clone)]
enum SharedBinding {
    Value(Box<Value>),
    Form(Vec<(String, Ty)>),
    Problem(Vec<(String, Ty)>),
}
type Shares = HashMap<String, SharedBinding>;
type ModuleCache = Rc<RefCell<HashMap<PathBuf, Shares>>>;
type LoadStack = Rc<RefCell<Vec<PathBuf>>>;
struct Env {
    parent: Option<EnvRef>,
    values: HashMap<String, Value>,
    forms: HashMap<String, Vec<(String, Ty)>>,
    problems: HashSet<String>,
    packages: HashMap<String, native::NativePackage>,
    loaded_packages: HashMap<String, Value>,
    extension_loaders: &'static [native::NativeRegister],
    next_extension_loader: usize,
    type_bindings: HashMap<String, Ty>,
    native_cleanups: Vec<native::NativeCleanup>,
    colour_output: bool,
    shares: Shares,
    module_cache: ModuleCache,
    load_stack: LoadStack,
    source: Option<PathBuf>,
    file_scope: bool,
}
fn scope(parent: Option<EnvRef>) -> EnvRef {
    let (module_cache, load_stack, source) = if let Some(parent) = &parent {
        let parent = parent.borrow();
        (
            parent.module_cache.clone(),
            parent.load_stack.clone(),
            parent.source.clone(),
        )
    } else {
        (
            Rc::new(RefCell::new(HashMap::new())),
            Rc::new(RefCell::new(Vec::new())),
            None,
        )
    };
    let environment = Rc::new(RefCell::new(Env {
        parent,
        values: HashMap::new(),
        forms: HashMap::new(),
        problems: HashSet::new(),
        packages: HashMap::new(),
        loaded_packages: HashMap::new(),
        extension_loaders: &[],
        next_extension_loader: 0,
        type_bindings: HashMap::new(),
        native_cleanups: Vec::new(),
        colour_output: false,
        shares: HashMap::new(),
        module_cache,
        load_stack,
        source,
        file_scope: false,
    }));
    profiler::count("scopes_created", 1);
    environment
}
fn source_scope(core: EnvRef, source: Option<PathBuf>) -> EnvRef {
    let environment = scope(Some(core));
    {
        let mut environment = environment.borrow_mut();
        environment.source = source;
        environment.file_scope = true;
    }
    environment
}
fn core_scope(environment: &EnvRef) -> EnvRef {
    let mut current = environment.clone();
    loop {
        let parent = current.borrow().parent.clone();
        match parent {
            Some(parent) => current = parent,
            None => return current,
        }
    }
}
fn shutdown_native_extensions(environment: &EnvRef) {
    let root = core_scope(environment);
    let cleanups = root.borrow().native_cleanups.clone();
    for cleanup in cleanups.into_iter().rev() {
        cleanup();
    }
}
fn root_scope() -> EnvRef {
    let root = scope(None);
    root.borrow_mut()
        .forms
        .insert("Problem".into(), vec![("message".into(), Ty::String)]);
    root.borrow_mut().problems.insert("Problem".into());
    root.borrow_mut()
        .values
        .insert("size".into(), val(Ty::Unit, Data::Builtin(Builtin::Size)));
    root.borrow_mut()
        .values
        .insert("unit".into(), val(Ty::Unit, Data::Unit));
    root.borrow_mut().extension_loaders = extensions::runtime_loaders();
    root
}
fn colour_output_enabled(environment: &EnvRef) -> bool {
    let mut current = Some(environment.clone());
    while let Some(scope) = current {
        if scope.borrow().colour_output {
            return true;
        }
        current = scope.borrow().parent.clone();
    }
    false
}
fn get(e: &EnvRef, n: &str) -> Option<Value> {
    let mut x = Some(e.clone());
    let mut hops = 0u64;
    while let Some(cur) = x {
        if let Some(v) = cur.borrow().values.get(n) {
            profiler::count("variable_reads", 1);
            profiler::count("variable_lookup_hops", hops);
            profiler::maximum("maximum_variable_lookup_hops", hops);
            return Some(v.clone());
        }
        hops += 1;
        x = cur.borrow().parent.clone()
    }
    profiler::count("failed_variable_reads", 1);
    None
}
fn package(e: &EnvRef, name: &str) -> Option<Value> {
    let mut current = Some(e.clone());
    while let Some(scope) = current {
        if let Some(value) = scope.borrow().loaded_packages.get(name).cloned() {
            return Some(value);
        }
        let mut definition = scope.borrow().packages.get(name).cloned();
        while definition.is_none() {
            let loader = {
                let mut environment = scope.borrow_mut();
                let loader = environment
                    .extension_loaders
                    .get(environment.next_extension_loader)
                    .copied();
                if loader.is_some() {
                    environment.next_extension_loader += 1;
                }
                loader
            };
            let Some(loader) = loader else { break };
            let mut registry = native::NativeRegistry::new(scope.clone());
            loader(&mut registry);
            definition = scope.borrow().packages.get(name).cloned();
        }
        if let Some(definition) = definition {
            let namespace = materialize_package(scope.clone(), definition);
            scope
                .borrow_mut()
                .loaded_packages
                .insert(name.into(), namespace.clone());
            return Some(namespace);
        }
        current = scope.borrow().parent.clone();
    }
    None
}
fn materialize_package(root: EnvRef, package: native::NativePackage) -> Value {
    let namespace = scope(Some(root));
    let (name, constants) = match package {
        native::NativePackage::Ordinary {
            name,
            functions,
            constants,
        } => {
            for function in functions {
                namespace.borrow_mut().values.insert(
                    function.name.into(),
                    val(
                        Ty::Unit,
                        Data::Builtin(Builtin::Native {
                            space: name,
                            name: function.name,
                            call: function.call,
                        }),
                    ),
                );
            }
            (name, constants)
        }
        native::NativePackage::Runtime {
            name,
            functions,
            constants,
        } => {
            for function in functions {
                namespace.borrow_mut().values.insert(
                    function.name.into(),
                    val(
                        Ty::Unit,
                        Data::Builtin(Builtin::NativeRuntime {
                            space: name,
                            name: function.name,
                            call: function.call,
                        }),
                    ),
                );
            }
            (name, constants)
        }
    };
    for (constant_name, constant) in constants {
        let value = match constant {
            native::NativeConstant::Int(value) => val(Ty::Int, Data::Int(value)),
            native::NativeConstant::Float(value) => val(Ty::Float, Data::Float(value)),
            native::NativeConstant::Bool(value) => val(Ty::Bool, Data::Bool(value)),
            native::NativeConstant::String(value) => val(Ty::String, Data::String(value.into())),
        };
        namespace
            .borrow_mut()
            .values
            .insert(constant_name.into(), value);
    }
    let _ = name;
    val(Ty::Unit, Data::Namespace(namespace))
}
fn load_shares(
    environment: &EnvRef,
    requested: &str,
    path: &str,
    line: usize,
) -> Result<SharedBinding> {
    let (source, cache, stack) = {
        let environment = environment.borrow();
        (
            environment.source.clone(),
            environment.module_cache.clone(),
            environment.load_stack.clone(),
        )
    };
    let source = source.ok_or_else(|| {
        Error::new(
            line,
            format!("borrow {requested} from {path:?} requires a source file"),
        )
    })?;
    let project_source = stack
        .borrow()
        .first()
        .cloned()
        .unwrap_or_else(|| source.clone());
    let canonical =
        project::resolve_stash(&project_source, &source, path).map_err(|mut error| {
            if error.line == 0 {
                error.line = line;
            }
            error
        })?;

    let cached = { cache.borrow().get(&canonical).cloned() };
    let shares = if let Some(shares) = cached {
        profiler::count("stash_cache_hits", 1);
        shares
    } else {
        profiler::count("stash_loads", 1);
        let cycle_position = {
            stack
                .borrow()
                .iter()
                .position(|loaded| loaded == &canonical)
        };
        if let Some(position) = cycle_position {
            let mut chain = stack.borrow()[position..]
                .iter()
                .map(|loaded| loaded.display().to_string())
                .collect::<Vec<_>>();
            chain.push(canonical.display().to_string());
            return Err(Error::new(
                line,
                format!("circular stash borrowing: {}", chain.join(" -> ")),
            ));
        }

        stack.borrow_mut().push(canonical.clone());
        let loaded = (|| {
            let source_text = fs::read_to_string(&canonical)
                .map_err(|error| Error::new(0, error.to_string()).with_source(&canonical))?;
            let stash = source_scope(core_scope(environment), Some(canonical.clone()));
            execute_in(&source_text, stash.clone(), Some(&canonical))?;
            let shares = stash.borrow().shares.clone();
            Ok(shares)
        })();
        stack.borrow_mut().pop();
        let shares = loaded?;
        cache.borrow_mut().insert(canonical.clone(), shares.clone());
        shares
    };

    shares.get(requested).cloned().ok_or_else(|| {
        Error::new(
            line,
            format!("stash {} does not share '{requested}'", canonical.display()),
        )
    })
}
fn set(e: &EnvRef, n: &str, v: Value) -> bool {
    let mut x = Some(e.clone());
    let mut hops = 0u64;
    while let Some(cur) = x {
        if cur.borrow().values.contains_key(n) {
            cur.borrow_mut().values.insert(n.into(), v);
            profiler::count("variable_writes", 1);
            profiler::count("variable_write_hops", hops);
            profiler::maximum("maximum_variable_write_hops", hops);
            return true;
        }
        hops += 1;
        x = cur.borrow().parent.clone()
    }
    false
}
fn form_def(e: &EnvRef, n: &str) -> Option<Vec<(String, Ty)>> {
    let mut x = Some(e.clone());
    while let Some(cur) = x {
        if let Some(v) = cur.borrow().forms.get(n) {
            return Some(v.clone());
        }
        x = cur.borrow().parent.clone()
    }
    None
}
fn is_problem_type(environment: &EnvRef, ty: &Ty) -> bool {
    let Ty::Named(name) = ty else {
        return false;
    };
    let mut current = Some(environment.clone());
    while let Some(scope) = current {
        if scope.borrow().problems.contains(name) {
            return true;
        }
        current = scope.borrow().parent.clone();
    }
    false
}
fn recovery_accepts(environment: &EnvRef, expected: &Ty, actual: &Ty) -> bool {
    same(expected, actual)
        || (matches!(expected, Ty::Named(name) if name == "Problem")
            && is_problem_type(environment, actual))
}
fn dynamic_problem_type(value: &Value) -> Option<Ty> {
    match &value.data {
        Data::Problem(name, _) => Some(Ty::Named(name.clone())),
        _ => None,
    }
}
fn base_problem(message: String) -> Value {
    val(
        Ty::Named("Problem".into()),
        Data::Problem(
            "Problem".into(),
            Rc::new(RefCell::new(BTreeMap::from([(
                "message".into(),
                val(Ty::String, Data::String(message)),
            )]))),
        ),
    )
}
fn val(ty: Ty, data: Data) -> Value {
    Value { ty, data }
}
fn byte_array(bytes: &[u8]) -> Value {
    val(
        Ty::Arr(Box::new(Ty::Int)),
        Data::Arr(Rc::new(RefCell::new(
            bytes
                .iter()
                .map(|byte| val(Ty::Int, Data::Int(i64::from(*byte))))
                .collect(),
        ))),
    )
}
fn optional_utf8(bytes: &[u8]) -> Value {
    let ty = Ty::Perchance(Box::new(Ty::String));
    match std::str::from_utf8(bytes) {
        Ok(text) => val(ty, Data::String(text.to_owned())),
        Err(_) => val(ty, Data::Naught),
    }
}
fn string_map(entries: &BTreeMap<String, String>) -> Value {
    let entries = entries
        .iter()
        .map(|(key, value)| {
            (
                format!("t:{key}"),
                val(Ty::String, Data::String(value.clone())),
            )
        })
        .collect();
    val(
        Ty::Map(Box::new(Ty::String), Box::new(Ty::String)),
        Data::Map(Rc::new(RefCell::new(entries))),
    )
}
fn key(v: &Value, line: usize) -> Result<String> {
    match &v.data {
        Data::Int(x) => Ok(format!("i:{x}")),
        Data::Float(_) => Err(Error::new(line, "float cannot be a map key")),
        Data::Bool(x) => Ok(format!("b:{x}")),
        Data::String(x) => Ok(format!("t:{x}")),
        _ => Err(Error::new(line, "map keys must be int, bool, or string")),
    }
}
fn same(a: &Ty, b: &Ty) -> bool {
    a == b
}
fn accepts(expected: &Ty, actual: &Ty) -> bool {
    if same(expected, actual) {
        return true;
    }
    match (expected, actual) {
        (Ty::Perchance(inner), Ty::Naught) => !matches!(inner.as_ref(), Ty::Naught),
        (Ty::Perchance(inner), actual) => accepts(inner, actual),
        (Ty::List(expected), Ty::List(actual)) | (Ty::Arr(expected), Ty::Arr(actual)) => {
            accepts(expected, actual)
        }
        (Ty::Map(expected_key, expected_value), Ty::Map(actual_key, actual_value)) => {
            same(expected_key, actual_key) && accepts(expected_value, actual_value)
        }
        _ => false,
    }
}
fn common_type(left: &Ty, right: &Ty) -> Option<Ty> {
    if same(left, right) {
        return Some(left.clone());
    }
    match (left, right) {
        (Ty::Naught, other) | (other, Ty::Naught) if !matches!(other, Ty::Naught) => {
            Some(match other {
                Ty::Perchance(_) => other.clone(),
                _ => Ty::Perchance(Box::new(other.clone())),
            })
        }
        (Ty::Perchance(inner), other) | (other, Ty::Perchance(inner)) if accepts(inner, other) => {
            Some(Ty::Perchance(inner.clone()))
        }
        (Ty::List(left), Ty::List(right)) => {
            common_type(left, right).map(|ty| Ty::List(Box::new(ty)))
        }
        (Ty::Arr(left), Ty::Arr(right)) => common_type(left, right).map(|ty| Ty::Arr(Box::new(ty))),
        _ => None,
    }
}
fn conform(mut value: Value, expected: &Ty) -> Option<Value> {
    if same(expected, &value.ty) {
        return Some(value);
    }
    match (expected, &value.ty, &value.data) {
        (Ty::Naught, Ty::Perchance(_), Data::Naught) => {
            value.ty = Ty::Naught;
            Some(value)
        }
        (expected, Ty::Perchance(inner), data)
            if same(expected, inner) && !matches!(data, Data::Naught) =>
        {
            value.ty = expected.clone();
            Some(value)
        }
        (Ty::Named(name), Ty::Named(_), Data::Problem(_, _)) if name == "Problem" => {
            value.ty = expected.clone();
            Some(value)
        }
        (Ty::Perchance(inner), actual, data)
            if matches!(actual, Ty::Naught)
                || accepts(inner, actual)
                || (matches!(inner.as_ref(), Ty::Named(name) if name == "Problem")
                    && matches!(data, Data::Problem(_, _))) =>
        {
            value.ty = expected.clone();
            Some(value)
        }
        (Ty::List(expected_item), Ty::List(_), Data::List(items)) => {
            let converted = items
                .borrow()
                .iter()
                .cloned()
                .map(|item| conform(item, expected_item))
                .collect::<Option<Vec<_>>>()?;
            Some(val(
                expected.clone(),
                Data::List(Rc::new(RefCell::new(converted))),
            ))
        }
        (Ty::Arr(expected_item), Ty::Arr(_), Data::Arr(items)) => {
            let converted = items
                .borrow()
                .iter()
                .cloned()
                .map(|item| conform(item, expected_item))
                .collect::<Option<Vec<_>>>()?;
            Some(val(
                expected.clone(),
                Data::Arr(Rc::new(RefCell::new(converted))),
            ))
        }
        (Ty::Map(expected_key, expected_value), Ty::Map(actual_key, _), Data::Map(items))
            if same(expected_key, actual_key) =>
        {
            let converted = items
                .borrow()
                .iter()
                .map(|(key, item)| Some((key.clone(), conform(item.clone(), expected_value)?)))
                .collect::<Option<BTreeMap<_, _>>>()?;
            Some(val(
                expected.clone(),
                Data::Map(Rc::new(RefCell::new(converted))),
            ))
        }
        _ => None,
    }
}
fn display(v: &Value) -> String {
    match &v.data {
        Data::Int(n) => n.to_string(),
        Data::Float(n) => n.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::String(s) => s.clone(),
        Data::Json(value) => value.to_string(),
        Data::Naught => "naught".into(),
        Data::List(xs) => format!(
            "[{}]",
            xs.borrow()
                .iter()
                .map(display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Data::Arr(xs) => format!(
            "@[{}]",
            xs.borrow()
                .iter()
                .map(display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Data::Map(xs) => format!(
            "#{{{}}}",
            xs.borrow()
                .iter()
                .map(|(key, value)| format!("{}: {}", display_map_key(key), display(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Data::UdpSocket(_) => "<udp socket>".into(),
        Data::UdpPacket { host, port, bytes } => {
            format!("<udp packet {host}:{port} {} bytes>", bytes.len())
        }
        Data::TcpListener(_) => "<tcp listener>".into(),
        Data::TcpStream(_) => "<tcp stream>".into(),
        Data::HttpResponse { status, body, .. } => {
            format!("<http response {status} {} bytes>", body.len())
        }
        Data::Form(_, fs) | Data::Problem(_, fs) => format!(
            "{{{}}}",
            fs.borrow()
                .iter()
                .map(|(k, v)| format!("{k}: {}", display(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Data::Function(_) => "<function>".into(),
        Data::Builtin(_) => "<builtin>".into(),
        Data::Namespace(_) => "<namespace>".into(),
        Data::Unit => "unit".into(),
    }
}
fn display_map_key(key: &str) -> String {
    match key.split_once(':') {
        Some(("t", value)) => format!("{value:?}"),
        Some(("i" | "b", value)) => value.to_owned(),
        _ => key.to_owned(),
    }
}
fn decode_map_key(encoded: &str, ty: &Ty, line: usize) -> Result<Value> {
    let (_, raw) = encoded
        .split_once(':')
        .ok_or_else(|| Error::new(line, "invalid map key"))?;
    match ty {
        Ty::String => Ok(val(Ty::String, Data::String(raw.to_owned()))),
        Ty::Int => raw
            .parse()
            .map(|value| val(Ty::Int, Data::Int(value)))
            .map_err(|_| Error::new(line, "invalid integer map key")),
        Ty::Bool => raw
            .parse()
            .map(|value| val(Ty::Bool, Data::Bool(value)))
            .map_err(|_| Error::new(line, "invalid boolean map key")),
        _ => Err(Error::new(line, "invalid map key type")),
    }
}
pub(crate) fn values_equal(left: &Value, right: &Value) -> bool {
    values_equal_inner(left, right, &mut HashSet::new())
}
fn values_equal_inner(
    left: &Value,
    right: &Value,
    visited: &mut HashSet<(usize, usize, u8)>,
) -> bool {
    let left_type = match &left.ty {
        Ty::Perchance(inner) => inner.as_ref(),
        ty => ty,
    };
    let right_type = match &right.ty {
        Ty::Perchance(inner) => inner.as_ref(),
        ty => ty,
    };
    match (&left.data, &right.data) {
        (Data::Naught, Data::Naught) => true,
        (Data::Naught, _) | (_, Data::Naught) => false,
        _ if !same(left_type, right_type) => false,
        (Data::Int(left), Data::Int(right)) => left == right,
        (Data::Float(left), Data::Float(right)) => left == right,
        (Data::Bool(left), Data::Bool(right)) => left == right,
        (Data::String(left), Data::String(right)) => left == right,
        (Data::Json(left), Data::Json(right)) => left == right,
        (Data::Unit, Data::Unit) => true,
        (Data::List(left), Data::List(right)) | (Data::Arr(left), Data::Arr(right)) => {
            let pair = (Rc::as_ptr(left) as usize, Rc::as_ptr(right) as usize, 1);
            if !visited.insert(pair) {
                return true;
            }
            let left = left.borrow();
            let right = right.borrow();
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| values_equal_inner(left, right, visited))
        }
        (Data::Map(left), Data::Map(right)) => {
            let pair = (Rc::as_ptr(left) as usize, Rc::as_ptr(right) as usize, 2);
            if !visited.insert(pair) {
                return true;
            }
            let left = left.borrow();
            let right = right.borrow();
            left.len() == right.len()
                && left.iter().all(|(key, left)| {
                    right
                        .get(key)
                        .is_some_and(|right| values_equal_inner(left, right, visited))
                })
        }
        (Data::Form(left_name, left), Data::Form(right_name, right))
        | (Data::Problem(left_name, left), Data::Problem(right_name, right)) => {
            if left_name != right_name {
                return false;
            }
            let pair = (Rc::as_ptr(left) as usize, Rc::as_ptr(right) as usize, 3);
            if !visited.insert(pair) {
                return true;
            }
            let left = left.borrow();
            let right = right.borrow();
            left.len() == right.len()
                && left.iter().all(|(field, left)| {
                    right
                        .get(field)
                        .is_some_and(|right| values_equal_inner(left, right, visited))
                })
        }
        _ => false,
    }
}
enum Flow {
    Normal,
    Return(Box<Value>),
    Enough(usize),
    Onwards(usize),
}
fn run(stmts: &[Stmt], env: EnvRef) -> Result<Flow> {
    for s in stmts {
        match exec(s, env.clone())? {
            Flow::Normal => {}
            flow => return Ok(flow),
        }
    }
    Ok(Flow::Normal)
}
fn exec(s: &Stmt, e: EnvRef) -> Result<Flow> {
    if !profiler::active() {
        return exec_inner(s, e);
    }
    let source = e.borrow().source.clone();
    profiler::span(
        "statement",
        statement_kind(s),
        source.as_deref(),
        statement_line(s),
        || exec_inner(s, e),
    )
}
fn raise_values(
    expressions: &[Expr],
    line: usize,
    environment: EnvRef,
    loud: bool,
) -> Result<Flow> {
    profiler::count("explicit_problems_raised", 1);
    let mut values = expressions
        .iter()
        .map(|expression| eval(expression, environment.clone()))
        .collect::<Result<Vec<_>>>()?;
    let custom = values.len() == 1 && dynamic_problem_type(&values[0]).is_some();
    let message = if custom {
        let Data::Problem(_, fields) = &values[0].data else {
            return Err(Error::new(line, "problem value has invalid runtime data"));
        };
        fields
            .borrow()
            .get("message")
            .map(display)
            .ok_or_else(|| Error::new(line, "problem value has no message field"))?
    } else {
        values.iter().map(display).collect::<Vec<_>>().join(" ")
    };
    let rendered = if loud {
        let label = if colour_output_enabled(&environment) {
            "\x1b[31mSCREAMING!!!\x1b[0m"
        } else {
            "SCREAMING!!!"
        };
        format!("{label} : {message}")
    } else {
        message.clone()
    };
    let mut error = Error::new(line, rendered);
    error.problem = Some(Box::new(if custom {
        values.remove(0)
    } else {
        base_problem(message)
    }));
    Err(error)
}
fn exec_inner(s: &Stmt, e: EnvRef) -> Result<Flow> {
    match s {
        Stmt::Let(n, ann, x, l) => {
            let concrete = ann
                .as_ref()
                .map(|expected| resolve_runtime_type(expected, &e));
            let mut v = if let Some(expected) = &concrete {
                eval_expected(x, expected, e.clone())?
            } else {
                eval(x, e.clone())?
            };
            if let Some(t) = &concrete {
                let actual = v.ty.clone();
                let Some(converted) = conform(v, t) else {
                    return Err(Error::new(
                        *l,
                        format!("{n} is declared as {t}, but value is {actual}"),
                    ));
                };
                v = converted;
            }
            e.borrow_mut().values.insert(n.clone(), v);
            Ok(Flow::Normal)
        }
        Stmt::Assign(target, operator, x, l) => {
            let target = resolve_assignment_target(target, e.clone(), *l)?;
            let expected = target.ty();
            let current = if operator.is_some() {
                Some(target.current(*l)?)
            } else {
                None
            };
            let right = eval_expected(x, &expected, e.clone())?;
            let v = if let Some(operator) = operator {
                binary(
                    current.expect("compound assignment captured its value"),
                    operator,
                    right,
                    *l,
                )?
            } else {
                right
            };
            target.write(v, &e, *l)?;
            Ok(Flow::Normal)
        }
        Stmt::Show(xs, _) => {
            let mut o = vec![];
            for x in xs {
                o.push(display(&eval(x, e.clone())?))
            }
            println!("{}", o.join(" "));
            Ok(Flow::Normal)
        }
        Stmt::Warn(xs, _) => {
            let mut output = vec![];
            for expression in xs {
                output.push(display(&eval(expression, e.clone())?));
            }
            let label = if colour_output_enabled(&e) {
                "\x1b[33mshouting!\x1b[0m"
            } else {
                "shouting!"
            };
            eprintln!("{label} : {}", output.join(" "));
            Ok(Flow::Normal)
        }
        Stmt::Raise(xs, line) => {
            profiler::count("explicit_problems_raised", 1);
            let mut values = vec![];
            for expression in xs {
                values.push(eval(expression, e.clone())?);
            }
            let custom = values.len() == 1 && dynamic_problem_type(&values[0]).is_some();
            let message = if custom {
                let Data::Problem(_, fields) = &values[0].data else {
                    return Err(Error::new(*line, "problem value has invalid runtime data"));
                };
                fields
                    .borrow()
                    .get("message")
                    .map(display)
                    .ok_or_else(|| Error::new(*line, "problem value has no message field"))?
            } else {
                values.iter().map(display).collect::<Vec<_>>().join(" ")
            };
            let label = if colour_output_enabled(&e) {
                "\x1b[31mSCREAMING!!!\x1b[0m"
            } else {
                "SCREAMING!!!"
            };
            let mut error = Error::new(*line, format!("{label} : {message}"));
            error.problem = Some(Box::new(if custom {
                values.remove(0)
            } else {
                base_problem(message)
            }));
            Err(error)
        }
        Stmt::QuietRaise(xs, line) => raise_values(xs, *line, e, false),
        Stmt::Attempt(body, recoveries, always, _) => {
            let attempted = run(body, scope(Some(e.clone())));
            let mut pending = match attempted {
                Err(error) if !error.clean_exit => {
                    let raised = error
                        .problem
                        .as_deref()
                        .cloned()
                        .unwrap_or_else(|| base_problem(error.message.clone()));
                    let raised_type =
                        dynamic_problem_type(&raised).unwrap_or_else(|| raised.ty.clone());
                    if let Some((name, _, body)) = recoveries
                        .iter()
                        .find(|(_, expected, _)| recovery_accepts(&e, expected, &raised_type))
                    {
                        profiler::count("problems_recovered", 1);
                        let recovered = scope(Some(e.clone()));
                        recovered.borrow_mut().values.insert(name.clone(), raised);
                        run(body, recovered)
                    } else {
                        Err(error)
                    }
                }
                result => result,
            };
            if let Some(body) = always {
                match run(body, scope(Some(e))) {
                    Ok(Flow::Normal) => {}
                    result => pending = result,
                }
            }
            pending
        }
        Stmt::If(c, y, n, l) => {
            let v = eval(c, e.clone())?;
            match v.data {
                Data::Bool(true) => {
                    profiler::count("if_true", 1);
                    run(y, scope(Some(e)))
                }
                Data::Bool(false) => {
                    profiler::count("if_false", 1);
                    run(n, scope(Some(e)))
                }
                _ => Err(Error::new(*l, "if condition must be bool")),
            }
        }
        Stmt::While(c, b, l) => {
            loop {
                match eval(c, e.clone())?.data {
                    Data::Bool(true) => {
                        profiler::count("while_iterations", 1);
                        match run(b, scope(Some(e.clone())))? {
                            Flow::Normal => {}
                            Flow::Return(value) => return Ok(Flow::Return(value)),
                            Flow::Enough(_) => break,
                            Flow::Onwards(_) => continue,
                        }
                    }
                    Data::Bool(false) => break,
                    _ => return Err(Error::new(*l, "aslongas condition must be bool")),
                }
            }
            Ok(Flow::Normal)
        }
        Stmt::For(n, x, b, l) => {
            let v = eval(x, e.clone())?;
            let items = match v.data {
                Data::List(items) | Data::Arr(items) => items.borrow().clone(),
                Data::Map(items) => {
                    let Ty::Map(key_type, _) = v.ty else {
                        unreachable!()
                    };
                    items
                        .borrow()
                        .keys()
                        .map(|key| decode_map_key(key, &key_type, *l))
                        .collect::<Result<Vec<_>>>()?
                }
                _ => return Err(Error::new(*l, "each expects a list, array, or map")),
            };
            profiler::count("collection_snapshot_elements", items.len() as u64);
            for item in items {
                profiler::count("each_iterations", 1);
                let child = scope(Some(e.clone()));
                child.borrow_mut().values.insert(n.clone(), item);
                match run(b, child)? {
                    Flow::Normal => {}
                    Flow::Return(value) => return Ok(Flow::Return(value)),
                    Flow::Enough(_) => break,
                    Flow::Onwards(_) => continue,
                }
            }
            Ok(Flow::Normal)
        }
        Stmt::Fn(n, declared_generics, p, r, b, definition_line) => {
            profiler::count("function_closures_created", 1);
            let generics = declared_generics.iter().cloned().collect();
            let f = val(
                Ty::Unit,
                Data::Function(Function {
                    name: n.clone(),
                    line: *definition_line,
                    params: p.clone(),
                    ret: r.clone(),
                    body: b.clone(),
                    closure: e.clone(),
                    source: e.borrow().source.clone(),
                    generics,
                    generic_bindings: HashMap::new(),
                }),
            );
            e.borrow_mut().values.insert(n.clone(), f);
            Ok(Flow::Normal)
        }
        Stmt::Return(x, _) => Ok(Flow::Return(Box::new(eval(x, e)?))),
        Stmt::Enough(line) => Ok(Flow::Enough(*line)),
        Stmt::Onwards(line) => Ok(Flow::Onwards(*line)),
        Stmt::Exit(line) => Err(Error::clean_exit(*line)),
        Stmt::Borrow(name, source, alias, line) => {
            let binding = alias.as_ref().unwrap_or(name);
            if let Some(path) = source {
                match load_shares(&e, name, path, *line)? {
                    SharedBinding::Value(value) => {
                        e.borrow_mut().values.insert(binding.clone(), *value);
                    }
                    SharedBinding::Form(fields) => {
                        if alias.is_some() {
                            return Err(Error::new(
                                *line,
                                "form aliases are not supported; borrow the form by its shared name",
                            ));
                        }
                        e.borrow_mut().forms.insert(binding.clone(), fields);
                    }
                    SharedBinding::Problem(fields) => {
                        if alias.is_some() {
                            return Err(Error::new(
                                *line,
                                "problem aliases are not supported; borrow the problem by its shared name",
                            ));
                        }
                        e.borrow_mut().forms.insert(binding.clone(), fields);
                        e.borrow_mut().problems.insert(binding.clone());
                    }
                }
                return Ok(Flow::Normal);
            }
            let value = package(&e, name).ok_or_else(|| {
                Error::new(*line, format!("no shipped runtime space named '{name}'"))
            })?;
            let mut environment = e.borrow_mut();
            environment.values.insert(name.clone(), value);
            if name == "LengText" {
                environment.colour_output = true;
            }
            Ok(Flow::Normal)
        }
        Stmt::Share(name, line) => {
            if !e.borrow().file_scope {
                return Err(Error::new(
                    *line,
                    "'share' is only valid at the top level of a stash",
                ));
            }
            let (value, form, problem) = {
                let environment = e.borrow();
                (
                    environment.values.get(name).cloned(),
                    environment.forms.get(name).cloned(),
                    environment.problems.contains(name),
                )
            };
            let shared = match (value, form) {
                (Some(_), Some(_)) => {
                    return Err(Error::new(
                        *line,
                        format!("'{name}' names both a value and a form"),
                    ));
                }
                (Some(value), None) => SharedBinding::Value(Box::new(value)),
                (None, Some(fields)) if problem => SharedBinding::Problem(fields),
                (None, Some(fields)) => SharedBinding::Form(fields),
                (None, None) => {
                    return Err(Error::new(
                        *line,
                        format!("cannot share unknown name '{name}'"),
                    ));
                }
            };
            e.borrow_mut().shares.insert(name.clone(), shared);
            Ok(Flow::Normal)
        }
        Stmt::Namespace(n, b, _) => {
            let ns = scope(Some(e.clone()));
            match run(b, ns.clone())? {
                Flow::Normal | Flow::Return(_) => {}
                Flow::Enough(line) => {
                    return Err(Error::new(line, "'enough' can only be used inside a loop"));
                }
                Flow::Onwards(line) => {
                    return Err(Error::new(line, "'onwards' can only be used inside a loop"));
                }
            }
            e.borrow_mut()
                .values
                .insert(n.clone(), val(Ty::Unit, Data::Namespace(ns)));
            Ok(Flow::Normal)
        }
        Stmt::Form(n, fs, _) => {
            e.borrow_mut().forms.insert(n.clone(), fs.clone());
            Ok(Flow::Normal)
        }
        Stmt::Problem(n, fs, _) => {
            e.borrow_mut().forms.insert(n.clone(), fs.clone());
            e.borrow_mut().problems.insert(n.clone());
            Ok(Flow::Normal)
        }
        Stmt::Expr(x) => {
            eval(x, e)?;
            Ok(Flow::Normal)
        }
    }
}

fn statement_line(statement: &Stmt) -> usize {
    match statement {
        Stmt::Let(_, _, _, line)
        | Stmt::Assign(_, _, _, line)
        | Stmt::Show(_, line)
        | Stmt::Warn(_, line)
        | Stmt::Raise(_, line)
        | Stmt::QuietRaise(_, line)
        | Stmt::Attempt(_, _, _, line)
        | Stmt::If(_, _, _, line)
        | Stmt::While(_, _, line)
        | Stmt::For(_, _, _, line)
        | Stmt::Fn(_, _, _, _, _, line)
        | Stmt::Return(_, line)
        | Stmt::Enough(line)
        | Stmt::Onwards(line)
        | Stmt::Exit(line)
        | Stmt::Borrow(_, _, _, line)
        | Stmt::Share(_, line)
        | Stmt::Namespace(_, _, line)
        | Stmt::Form(_, _, line)
        | Stmt::Problem(_, _, line) => *line,
        Stmt::Expr(expression) => expression.line(),
    }
}

fn statement_kind(statement: &Stmt) -> &'static str {
    match statement {
        Stmt::Let(..) => "declaration",
        Stmt::Assign(..) => "assignment",
        Stmt::Show(..) => "say",
        Stmt::Warn(..) => "shout",
        Stmt::Raise(..) => "scream",
        Stmt::QuietRaise(..) => "raise",
        Stmt::Attempt(..) => "attempt",
        Stmt::If(..) => "if",
        Stmt::While(..) => "aslongas",
        Stmt::For(..) => "each",
        Stmt::Fn(..) => "given",
        Stmt::Return(..) => "ret",
        Stmt::Enough(..) => "enough",
        Stmt::Onwards(..) => "onwards",
        Stmt::Exit(..) => "exit",
        Stmt::Borrow(..) => "borrow",
        Stmt::Share(..) => "share",
        Stmt::Namespace(..) => "space",
        Stmt::Form(..) => "form",
        Stmt::Problem(..) => "problem",
        Stmt::Expr(..) => "expression",
    }
}
enum AssignmentTarget {
    Name(String, Box<Value>),
    Sequence(Rc<RefCell<Vec<Value>>>, usize, Ty, &'static str),
    Map(Rc<RefCell<BTreeMap<String, Value>>>, String, Ty),
    Field(Rc<RefCell<BTreeMap<String, Value>>>, String, Ty),
}
impl AssignmentTarget {
    fn ty(&self) -> Ty {
        match self {
            Self::Name(_, value) => value.ty.clone(),
            Self::Sequence(_, _, ty, _) | Self::Map(_, _, ty) | Self::Field(_, _, ty) => ty.clone(),
        }
    }
    fn current(&self, line: usize) -> Result<Value> {
        match self {
            Self::Name(_, value) => Ok((**value).clone()),
            Self::Sequence(items, index, _, kind) => items
                .borrow()
                .get(*index)
                .cloned()
                .ok_or_else(|| Error::new(line, format!("{kind} index out of bounds"))),
            Self::Map(entries, key, _) => entries
                .borrow()
                .get(key)
                .cloned()
                .ok_or_else(|| Error::new(line, "map key not found")),
            Self::Field(fields, name, _) => fields
                .borrow()
                .get(name)
                .cloned()
                .ok_or_else(|| Error::new(line, format!("unknown field '{name}'"))),
        }
    }
    fn write(self, value: Value, environment: &EnvRef, line: usize) -> Result<()> {
        let expected = self.ty();
        let actual = value.ty.clone();
        let Some(value) = conform(value, &expected) else {
            return Err(Error::new(
                line,
                format!("assignment expects {expected}, got {actual}"),
            ));
        };
        match self {
            Self::Name(name, _) => {
                if set(environment, &name, value) {
                    Ok(())
                } else {
                    Err(Error::new(line, format!("unknown name '{name}'")))
                }
            }
            Self::Sequence(items, index, _, kind) => {
                let mut items = items.borrow_mut();
                let slot = items
                    .get_mut(index)
                    .ok_or_else(|| Error::new(line, format!("{kind} index out of bounds")))?;
                *slot = value;
                Ok(())
            }
            Self::Map(entries, key, _) => {
                entries.borrow_mut().insert(key, value);
                Ok(())
            }
            Self::Field(fields, name, _) => {
                fields.borrow_mut().insert(name, value);
                Ok(())
            }
        }
    }
}
fn resolve_assignment_target(
    target: &Expr,
    environment: EnvRef,
    line: usize,
) -> Result<AssignmentTarget> {
    match target {
        Expr::Name(name, _) => {
            let value = get(&environment, name)
                .ok_or_else(|| Error::new(line, format!("unknown name '{name}'")))?;
            Ok(AssignmentTarget::Name(name.clone(), Box::new(value)))
        }
        Expr::Index(base, index, _) => {
            let owner = eval(base, environment.clone())?;
            let index = eval(index, environment)?;
            match (owner.ty, owner.data) {
                (Ty::List(element), Data::List(items)) => {
                    let Data::Int(position) = index.data else {
                        return Err(Error::new(line, "list or array index must be int"));
                    };
                    if position < 0 {
                        return Err(Error::new(line, "index out of bounds"));
                    }
                    Ok(AssignmentTarget::Sequence(
                        items,
                        position as usize,
                        *element,
                        "list",
                    ))
                }
                (Ty::Arr(element), Data::Arr(items)) => {
                    let Data::Int(position) = index.data else {
                        return Err(Error::new(line, "list or array index must be int"));
                    };
                    if position < 0 {
                        return Err(Error::new(line, "index out of bounds"));
                    }
                    Ok(AssignmentTarget::Sequence(
                        items,
                        position as usize,
                        *element,
                        "array",
                    ))
                }
                (Ty::Map(key_type, value_type), Data::Map(entries)) => {
                    if !same(&key_type, &index.ty) {
                        return Err(Error::new(
                            line,
                            format!("map key expects {key_type}, got {}", index.ty),
                        ));
                    }
                    Ok(AssignmentTarget::Map(
                        entries,
                        key(&index, line)?,
                        *value_type,
                    ))
                }
                _ => Err(Error::new(
                    line,
                    "only lists, arrays, and maps have mutable slots",
                )),
            }
        }
        Expr::Field(base, field, _) => match eval(base, environment)?.data {
            Data::Form(_, fields) | Data::Problem(_, fields) => {
                let ty = fields
                    .borrow()
                    .get(field)
                    .map(|value| value.ty.clone())
                    .ok_or_else(|| Error::new(line, format!("unknown field '{field}'")))?;
                Ok(AssignmentTarget::Field(fields, field.clone(), ty))
            }
            _ => Err(Error::new(line, "only form fields can be assigned")),
        },
        _ => Err(Error::new(line, "invalid assignment target")),
    }
}
#[allow(dead_code)]
fn assignment_type(target: &Expr, environment: &EnvRef, line: usize) -> Result<Ty> {
    match target {
        Expr::Name(name, _) => get(environment, name)
            .map(|value| value.ty)
            .ok_or_else(|| Error::new(line, format!("unknown name '{name}'"))),
        Expr::Index(base, _, _) => {
            let value = eval(base, environment.clone())?;
            match value.ty {
                Ty::List(item) | Ty::Arr(item) => Ok(*item),
                Ty::Map(_, value) => Ok(*value),
                _ => Err(Error::new(
                    line,
                    "only lists, arrays, and maps have mutable slots",
                )),
            }
        }
        Expr::Field(base, field, _) => match eval(base, environment.clone())?.data {
            Data::Form(_, fields) | Data::Problem(_, fields) => fields
                .borrow()
                .get(field)
                .map(|value| value.ty.clone())
                .ok_or_else(|| Error::new(line, format!("unknown field '{field}'"))),
            _ => Err(Error::new(line, "only form fields can be assigned")),
        },
        _ => Err(Error::new(
            line,
            "assignment target must be a variable or array or map slot",
        )),
    }
}
#[allow(dead_code)]
fn assign(t: &Expr, mut v: Value, e: EnvRef, l: usize) -> Result<()> {
    match t {
        Expr::Name(n, _) => {
            let old = get(&e, n).ok_or_else(|| Error::new(l, format!("unknown name '{n}'")))?;
            let actual = v.ty.clone();
            let Some(converted) = conform(v, &old.ty) else {
                return Err(Error::new(
                    l,
                    format!("cannot assign {actual} to {n} ({})", old.ty),
                ));
            };
            v = converted;
            if !set(&e, n, v) {
                unreachable!()
            }
            Ok(())
        }
        Expr::Index(base, index, _) => {
            let old = eval(base, e.clone())?;
            let position = eval(index, e.clone())?;
            match old.ty.clone() {
                Ty::List(element_type) | Ty::Arr(element_type) => {
                    let Data::Int(position) = position.data else {
                        return Err(Error::new(l, "array index must be int"));
                    };
                    if position < 0 {
                        return Err(Error::new(l, "array index out of bounds"));
                    }
                    let actual = v.ty.clone();
                    let Some(converted) = conform(v, &element_type) else {
                        return Err(Error::new(
                            l,
                            format!("array slot expects {element_type}, got {actual}"),
                        ));
                    };
                    v = converted;
                    let items = match old.data {
                        Data::List(items) | Data::Arr(items) => items,
                        _ => unreachable!(),
                    };
                    let mut items = items.borrow_mut();
                    let slot = items
                        .get_mut(position as usize)
                        .ok_or_else(|| Error::new(l, "array index out of bounds"))?;
                    *slot = v;
                    Ok(())
                }
                Ty::Map(key_type, value_type) => {
                    if !same(&key_type, &position.ty) {
                        return Err(Error::new(
                            l,
                            format!("map key expects {key_type}, got {}", position.ty),
                        ));
                    }
                    let actual = v.ty.clone();
                    let Some(converted) = conform(v, &value_type) else {
                        return Err(Error::new(
                            l,
                            format!("map value expects {value_type}, got {actual}"),
                        ));
                    };
                    v = converted;
                    let Data::Map(items) = old.data else {
                        unreachable!()
                    };
                    items.borrow_mut().insert(key(&position, l)?, v);
                    Ok(())
                }
                _ => Err(Error::new(
                    l,
                    "only lists, arrays, and maps have mutable slots",
                )),
            }
        }
        Expr::Field(base, field, _) => {
            let owner = eval(base, e)?;
            let fields = match owner.data {
                Data::Form(_, fields) | Data::Problem(_, fields) => fields,
                _ => return Err(Error::new(l, "only form fields can be assigned")),
            };
            let expected = fields
                .borrow()
                .get(field)
                .map(|value| value.ty.clone())
                .ok_or_else(|| Error::new(l, format!("unknown field '{field}'")))?;
            let actual = v.ty.clone();
            let Some(value) = conform(v, &expected) else {
                return Err(Error::new(
                    l,
                    format!("field '{field}' expects {expected}, got {actual}"),
                ));
            };
            fields.borrow_mut().insert(field.clone(), value);
            Ok(())
        }
        _ => Err(Error::new(
            l,
            "assignment target must be a variable or array or map slot",
        )),
    }
}
fn eval(x: &Expr, e: EnvRef) -> Result<Value> {
    if !profiler::active() {
        return eval_inner(x, e);
    }
    let source = e.borrow().source.clone();
    let kind = expression_kind(x);
    profiler::span("expression", &kind, source.as_deref(), x.line(), || {
        eval_inner(x, e)
    })
}
fn eval_inner(x: &Expr, e: EnvRef) -> Result<Value> {
    let l = x.line();
    match x {
        Expr::Int(n, _) => Ok(val(Ty::Int, Data::Int(*n))),
        Expr::Float(n, _) => Ok(val(Ty::Float, Data::Float(*n))),
        Expr::Bool(b, _) => Ok(val(Ty::Bool, Data::Bool(*b))),
        Expr::String(s, _) => Ok(val(Ty::String, Data::String(s.clone()))),
        Expr::Naught(_) => Ok(val(Ty::Naught, Data::Naught)),
        Expr::Name(n, _) => get(&e, n).ok_or_else(|| Error::new(l, format!("unknown name '{n}'"))),
        Expr::Declare(name, annotation, expression, _) => {
            let concrete = annotation
                .as_ref()
                .map(|expected| resolve_runtime_type(expected, &e));
            let mut value = if let Some(expected) = &concrete {
                eval_expected(expression, expected, e.clone())?
            } else {
                eval(expression, e.clone())?
            };
            if let Some(expected) = &concrete {
                let actual = value.ty.clone();
                let Some(converted) = conform(value, expected) else {
                    return Err(Error::new(
                        l,
                        format!("{name} is declared as {expected}, but value is {actual}"),
                    ));
                };
                value = converted;
            }
            e.borrow_mut().values.insert(name.clone(), value.clone());
            Ok(value)
        }
        Expr::List(xs, _) => {
            profiler::count("lists_created", 1);
            profiler::count("list_literal_elements", xs.len() as u64);
            let mut vs = vec![];
            let mut ty = None;
            for x in xs {
                let v = eval(x, e.clone())?;
                if let Some(t) = &ty {
                    let Some(merged) = common_type(t, &v.ty) else {
                        return Err(Error::new(
                            l,
                            format!("list contains both {t} and {}", v.ty),
                        ));
                    };
                    ty = Some(merged);
                } else {
                    ty = Some(v.ty.clone())
                }
                vs.push(v)
            }
            // Static checking guarantees that a source-level empty literal has
            // an expected collection type. Unit is only a transient runtime
            // placeholder for native calls, whose typed signature supplied
            // that context before execution.
            let t = ty.unwrap_or(Ty::Unit);
            let vs = vs
                .into_iter()
                .map(|value| conform(value, &t).expect("common list type must accept every item"))
                .collect();
            Ok(val(
                Ty::List(Box::new(t)),
                Data::List(Rc::new(RefCell::new(vs))),
            ))
        }
        Expr::Arr(xs, _) => {
            profiler::count("arrays_created", 1);
            profiler::count("array_literal_elements", xs.len() as u64);
            let mut vs = vec![];
            let mut ty = None;
            for x in xs {
                let v = eval(x, e.clone())?;
                if let Some(t) = &ty {
                    let Some(merged) = common_type(t, &v.ty) else {
                        return Err(Error::new(
                            l,
                            format!("array contains both {t} and {}", v.ty),
                        ));
                    };
                    ty = Some(merged);
                } else {
                    ty = Some(v.ty.clone())
                }
                vs.push(v);
            }
            let t = ty.unwrap_or(Ty::Unit);
            let vs = vs
                .into_iter()
                .map(|value| conform(value, &t).expect("common array type must accept every item"))
                .collect();
            Ok(val(
                Ty::Arr(Box::new(t)),
                Data::Arr(Rc::new(RefCell::new(vs))),
            ))
        }
        Expr::Map(xs, _) => {
            profiler::count("maps_created", 1);
            profiler::count("map_literal_entries", xs.len() as u64);
            let mut m = BTreeMap::new();
            let (mut kt, mut vt) = (None, None);
            for (a, b) in xs {
                let k = eval(a, e.clone())?;
                let v = eval(b, e.clone())?;
                if let Some(t) = &kt {
                    if !same(t, &k.ty) {
                        return Err(Error::new(l, "map keys have different types"));
                    }
                } else {
                    kt = Some(k.ty.clone())
                }
                if let Some(t) = &vt {
                    let Some(merged) = common_type(t, &v.ty) else {
                        return Err(Error::new(l, "map values have different types"));
                    };
                    vt = Some(merged);
                } else {
                    vt = Some(v.ty.clone())
                }
                m.insert(key(&k, l)?, v);
            }
            let (k, v) = (kt.unwrap_or(Ty::Unit), vt.unwrap_or(Ty::Unit));
            let m = m
                .into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        conform(value, &v).expect("common map type must accept every value"),
                    )
                })
                .collect();
            Ok(val(
                Ty::Map(Box::new(k), Box::new(v)),
                Data::Map(Rc::new(RefCell::new(m))),
            ))
        }
        Expr::Form(n, fs, _) => {
            profiler::count("forms_created", 1);
            profiler::count("form_fields_initialized", fs.len() as u64);
            let def =
                form_def(&e, n).ok_or_else(|| Error::new(l, format!("unknown form '{n}'")))?;
            let mut m = BTreeMap::new();
            for (field, expression) in fs {
                let t = def
                    .iter()
                    .find(|(candidate, _)| candidate == field)
                    .map(|(_, ty)| ty)
                    .ok_or_else(|| Error::new(l, format!("unknown field '{field}'")))?;
                if m.contains_key(field) {
                    return Err(Error::new(l, format!("duplicate field '{field}'")));
                }
                let v = eval_expected(expression, t, e.clone())?;
                let actual = v.ty.clone();
                let Some(v) = conform(v, t) else {
                    return Err(Error::new(
                        l,
                        format!("field '{field}' expects {t}, got {actual}"),
                    ));
                };
                m.insert(field.clone(), v);
            }
            if let Some((missing, _)) = def.iter().find(|(field, _)| !m.contains_key(field)) {
                return Err(Error::new(l, format!("missing field '{missing}'")));
            }
            let data = if is_problem_type(&e, &Ty::Named(n.clone())) {
                Data::Problem(n.clone(), Rc::new(RefCell::new(m)))
            } else {
                Data::Form(n.clone(), Rc::new(RefCell::new(m)))
            };
            Ok(val(Ty::Named(n.clone()), data))
        }
        Expr::Unary(op, a, _) => {
            let v = eval(a, e)?;
            match (op.as_str(), v.data) {
                ("-", Data::Int(n)) => n
                    .checked_neg()
                    .map(|value| val(Ty::Int, Data::Int(value)))
                    .ok_or_else(|| Error::new(l, "integer overflow in unary '-'")),
                ("-", Data::Float(n)) => Ok(val(Ty::Float, Data::Float(-n))),
                ("!", Data::Bool(b)) => Ok(val(Ty::Bool, Data::Bool(!b))),
                ("~", Data::Int(n)) => Ok(val(Ty::Int, Data::Int(!n))),
                _ => Err(Error::new(
                    l,
                    format!("'{op}' cannot be applied to {}", v.ty),
                )),
            }
        }
        Expr::Binary(a, op, b, _) => {
            let left = eval(a, e.clone())?;
            match (&left.data, op.as_str()) {
                (Data::Bool(false), "&&") => return Ok(val(Ty::Bool, Data::Bool(false))),
                (Data::Bool(true), "||") => return Ok(val(Ty::Bool, Data::Bool(true))),
                _ => {}
            }
            let right = eval(b, e)?;
            binary(left, op, right, l)
        }
        Expr::Call(f, args, _) => call(eval(f, e.clone())?, args, e, l),
        Expr::Cast(value, ty, _) => cast(eval(value, e)?, ty, l),
        Expr::Index(a, i, _) => {
            let c = eval(a, e.clone())?;
            let idx = eval(i, e)?;
            let cty = c.ty.clone();
            match c.data {
                Data::List(xs) => match idx.data {
                    Data::Int(n) if n >= 0 => xs
                        .borrow()
                        .get(n as usize)
                        .cloned()
                        .ok_or_else(|| Error::new(l, "list index out of bounds")),
                    _ => Err(Error::new(l, "list index must be int")),
                },
                Data::Arr(xs) => match idx.data {
                    Data::Int(n) if n >= 0 => xs
                        .borrow()
                        .get(n as usize)
                        .cloned()
                        .ok_or_else(|| Error::new(l, "array index out of bounds")),
                    _ => Err(Error::new(l, "array index must be int")),
                },
                Data::Map(m) => {
                    let Ty::Map(key_ty, _) = cty else {
                        unreachable!()
                    };
                    if !same(&key_ty, &idx.ty) {
                        return Err(Error::new(
                            l,
                            format!("map key expects {key_ty}, got {}", idx.ty),
                        ));
                    }
                    m.borrow()
                        .get(&key(&idx, l)?)
                        .cloned()
                        .ok_or_else(|| Error::new(l, "map key not found"))
                }
                Data::String(string) => match idx.data {
                    Data::Int(index) if index >= 0 => string
                        .chars()
                        .nth(index as usize)
                        .map(|character| val(Ty::String, Data::String(character.to_string())))
                        .ok_or_else(|| Error::new(l, "string index out of bounds")),
                    _ => Err(Error::new(l, "string index must be int")),
                },
                _ => Err(Error::new(
                    l,
                    "only string, lists, arrays, and maps can be indexed",
                )),
            }
        }
        Expr::Field(a, n, _) => {
            let v = eval(a, e)?;
            match v.data {
                Data::Form(_, m) | Data::Problem(_, m) => m
                    .borrow()
                    .get(n)
                    .cloned()
                    .ok_or_else(|| Error::new(l, format!("unknown field '{n}'"))),
                Data::UdpPacket { host, port, bytes } => match n.as_str() {
                    "host" => Ok(val(Ty::String, Data::String(host))),
                    "port" => Ok(val(Ty::Int, Data::Int(port))),
                    "bytes" => Ok(byte_array(&bytes)),
                    "text" => Ok(optional_utf8(&bytes)),
                    _ => Err(Error::new(l, format!("udp_packet has no '{n}' field"))),
                },
                Data::HttpResponse {
                    status,
                    reason,
                    version,
                    headers,
                    body,
                } => match n.as_str() {
                    "status" => Ok(val(Ty::Int, Data::Int(status))),
                    "reason" => Ok(val(Ty::String, Data::String(reason))),
                    "version" => Ok(val(Ty::String, Data::String(version))),
                    "headers" => Ok(string_map(&headers)),
                    "body" => Ok(byte_array(&body)),
                    "text" => Ok(optional_utf8(&body)),
                    _ => Err(Error::new(l, format!("http_response has no '{n}' field"))),
                },
                Data::Namespace(ns) => {
                    get(&ns, n).ok_or_else(|| Error::new(l, format!("namespace has no '{n}'")))
                }
                _ => Err(Error::new(l, "field access requires a form or namespace")),
            }
        }
    }
}
fn expression_kind(expression: &Expr) -> String {
    match expression {
        Expr::Int(..) => "int literal".into(),
        Expr::Float(..) => "float literal".into(),
        Expr::Bool(..) => "bool literal".into(),
        Expr::String(..) => "string literal".into(),
        Expr::Naught(..) => "naught literal".into(),
        Expr::Name(..) => "name lookup".into(),
        Expr::Declare(..) => "declaration expression".into(),
        Expr::List(..) => "list literal".into(),
        Expr::Arr(..) => "array literal".into(),
        Expr::Map(..) => "map literal".into(),
        Expr::Form(..) => "form construction".into(),
        Expr::Unary(operator, ..) => format!("unary {operator}"),
        Expr::Binary(_, operator, _, _) => format!("binary {operator}"),
        Expr::Call(..) => "call".into(),
        Expr::Cast(..) => "conversion".into(),
        Expr::Index(..) => "index".into(),
        Expr::Field(..) => "field access".into(),
    }
}
fn resolve_runtime_type(ty: &Ty, environment: &EnvRef) -> Ty {
    match ty {
        Ty::Named(name) => {
            let mut current = Some(environment.clone());
            while let Some(scope) = current {
                if let Some(concrete) = scope.borrow().type_bindings.get(name).cloned() {
                    return concrete;
                }
                current = scope.borrow().parent.clone();
            }
            ty.clone()
        }
        Ty::Perchance(inner) => Ty::Perchance(Box::new(resolve_runtime_type(inner, environment))),
        Ty::List(inner) => Ty::List(Box::new(resolve_runtime_type(inner, environment))),
        Ty::Arr(inner) => Ty::Arr(Box::new(resolve_runtime_type(inner, environment))),
        Ty::Map(key, value) => Ty::Map(
            Box::new(resolve_runtime_type(key, environment)),
            Box::new(resolve_runtime_type(value, environment)),
        ),
        _ => ty.clone(),
    }
}
fn eval_expected(expression: &Expr, expected: &Ty, environment: EnvRef) -> Result<Value> {
    let expected = resolve_runtime_type(expected, &environment);
    let line = expression.line();
    let contextual = match (expression, &expected) {
        (Expr::List(items, _), Ty::List(item_type)) => {
            profiler::count("lists_created", 1);
            profiler::count("list_literal_elements", items.len() as u64);
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval_expected(item, item_type, environment.clone())?);
            }
            Some(val(
                expected.clone(),
                Data::List(Rc::new(RefCell::new(values))),
            ))
        }
        (Expr::Arr(items, _), Ty::Arr(item_type)) => {
            profiler::count("arrays_created", 1);
            profiler::count("array_literal_elements", items.len() as u64);
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval_expected(item, item_type, environment.clone())?);
            }
            Some(val(
                expected.clone(),
                Data::Arr(Rc::new(RefCell::new(values))),
            ))
        }
        (Expr::Map(items, _), Ty::Map(key_type, value_type)) => {
            profiler::count("maps_created", 1);
            profiler::count("map_literal_entries", items.len() as u64);
            let mut values = BTreeMap::new();
            for (key_expression, value_expression) in items {
                let key_value = eval_expected(key_expression, key_type, environment.clone())?;
                let value = eval_expected(value_expression, value_type, environment.clone())?;
                values.insert(key(&key_value, line)?, value);
            }
            Some(val(
                expected.clone(),
                Data::Map(Rc::new(RefCell::new(values))),
            ))
        }
        _ => None,
    };
    if let Some(value) = contextual {
        return Ok(value);
    }
    let value = eval(expression, environment)?;
    let actual = value.ty.clone();
    conform(value, &expected).ok_or_else(|| {
        Error::new(
            line,
            format!("expected {expected}, but expression is {actual}"),
        )
    })
}
fn cast(value: Value, target: &Ty, line: usize) -> Result<Value> {
    if let Some(converted) = conform(value.clone(), target) {
        return Ok(converted);
    }
    if let Ty::Perchance(inner) = &value.ty {
        if matches!(value.data, Data::Naught) {
            return Err(Error::new(
                line,
                format!("cannot pour naught into {target}"),
            ));
        }
        if inner.as_ref() == target {
            return Ok(val(target.clone(), value.data));
        }
    }
    match (value.data, target) {
        (Data::String(string), Ty::Int) => {
            let number = string
                .parse::<i64>()
                .map_err(|_| Error::new(line, format!("cannot turn {string:?} into int")))?;
            Ok(val(Ty::Int, Data::Int(number)))
        }
        (Data::Int(number), Ty::String) => Ok(val(Ty::String, Data::String(number.to_string()))),
        (Data::String(string), Ty::Float) => {
            let number = string
                .parse::<f64>()
                .map_err(|_| Error::new(line, format!("cannot turn {string:?} into float")))?;
            Ok(val(Ty::Float, Data::Float(number)))
        }
        (Data::Float(number), Ty::String) => Ok(val(Ty::String, Data::String(number.to_string()))),
        (Data::Int(number), Ty::Float) => Ok(val(Ty::Float, Data::Float(number as f64))),
        (Data::Float(number), Ty::Int) => Ok(val(Ty::Int, Data::Int(number as i64))),
        (Data::String(string), Ty::Bool) => match string.as_str() {
            "true" => Ok(val(Ty::Bool, Data::Bool(true))),
            "false" => Ok(val(Ty::Bool, Data::Bool(false))),
            _ => Err(Error::new(
                line,
                format!("cannot turn {string:?} into bool"),
            )),
        },
        (Data::Bool(boolean), Ty::String) => Ok(val(Ty::String, Data::String(boolean.to_string()))),
        (_, target) => Err(Error::new(
            line,
            format!("cannot turn {} into {target}", value.ty),
        )),
    }
}
fn binary(a: Value, op: &str, b: Value, l: usize) -> Result<Value> {
    match (&a.data, op, &b.data) {
        (Data::Int(x), "+", Data::Int(y)) => checked_int(x.checked_add(*y), op, l),
        (Data::Int(x), "-", Data::Int(y)) => checked_int(x.checked_sub(*y), op, l),
        (Data::Int(x), "*", Data::Int(y)) => checked_int(x.checked_mul(*y), op, l),
        (Data::Int(_), "/", Data::Int(0)) | (Data::Int(_), "%", Data::Int(0)) => {
            Err(Error::new(l, "division by zero"))
        }
        (Data::Int(x), "/", Data::Int(y)) => checked_int(x.checked_div(*y), op, l),
        (Data::Int(x), "%", Data::Int(y)) => checked_int(x.checked_rem(*y), op, l),
        (Data::Int(x), "&", Data::Int(y)) => Ok(val(Ty::Int, Data::Int(x & y))),
        (Data::Int(x), "|", Data::Int(y)) => Ok(val(Ty::Int, Data::Int(x | y))),
        (Data::Int(x), "^", Data::Int(y)) => Ok(val(Ty::Int, Data::Int(x ^ y))),
        (Data::Int(_), "<<" | ">>", Data::Int(y)) if !(0..64).contains(y) => {
            Err(Error::new(l, "bit shift must be between 0 and 63"))
        }
        (Data::Int(x), "<<", Data::Int(y)) => {
            let shifted = (*x as i128) << (*y as u32);
            i64::try_from(shifted)
                .map(|value| val(Ty::Int, Data::Int(value)))
                .map_err(|_| Error::new(l, "integer overflow in '<<'"))
        }
        (Data::Int(x), ">>", Data::Int(y)) => Ok(val(Ty::Int, Data::Int(x >> y))),
        (Data::Float(x), "+", Data::Float(y)) => Ok(val(Ty::Float, Data::Float(x + y))),
        (Data::Float(x), "-", Data::Float(y)) => Ok(val(Ty::Float, Data::Float(x - y))),
        (Data::Float(x), "*", Data::Float(y)) => Ok(val(Ty::Float, Data::Float(x * y))),
        (Data::Float(_), "/", Data::Float(y)) if *y == 0.0 => {
            Err(Error::new(l, "division by zero"))
        }
        (Data::Float(x), "/", Data::Float(y)) => Ok(val(Ty::Float, Data::Float(x / y))),
        (Data::String(x), "+", Data::String(y)) => {
            profiler::count("text_concatenations", 1);
            profiler::count("text_bytes_copied", (x.len() + y.len()) as u64);
            Ok(val(Ty::String, Data::String(format!("{x}{y}"))))
        }
        (Data::Bool(x), "&&", Data::Bool(y)) => Ok(val(Ty::Bool, Data::Bool(*x && *y))),
        (Data::Bool(x), "||", Data::Bool(y)) => Ok(val(Ty::Bool, Data::Bool(*x || *y))),
        (_, "==", _) => Ok(val(Ty::Bool, Data::Bool(values_equal(&a, &b)))),
        (_, "!=", _) => Ok(val(Ty::Bool, Data::Bool(!values_equal(&a, &b)))),
        (Data::Int(x), "<", Data::Int(y)) => Ok(val(Ty::Bool, Data::Bool(x < y))),
        (Data::Int(x), "<=", Data::Int(y)) => Ok(val(Ty::Bool, Data::Bool(x <= y))),
        (Data::Int(x), ">", Data::Int(y)) => Ok(val(Ty::Bool, Data::Bool(x > y))),
        (Data::Int(x), ">=", Data::Int(y)) => Ok(val(Ty::Bool, Data::Bool(x >= y))),
        (Data::Float(x), "<", Data::Float(y)) => Ok(val(Ty::Bool, Data::Bool(x < y))),
        (Data::Float(x), "<=", Data::Float(y)) => Ok(val(Ty::Bool, Data::Bool(x <= y))),
        (Data::Float(x), ">", Data::Float(y)) => Ok(val(Ty::Bool, Data::Bool(x > y))),
        (Data::Float(x), ">=", Data::Float(y)) => Ok(val(Ty::Bool, Data::Bool(x >= y))),
        (Data::String(x), "<", Data::String(y)) => Ok(val(Ty::Bool, Data::Bool(x < y))),
        (Data::String(x), "<=", Data::String(y)) => Ok(val(Ty::Bool, Data::Bool(x <= y))),
        (Data::String(x), ">", Data::String(y)) => Ok(val(Ty::Bool, Data::Bool(x > y))),
        (Data::String(x), ">=", Data::String(y)) => Ok(val(Ty::Bool, Data::Bool(x >= y))),
        _ => Err(Error::new(
            l,
            format!("cannot use '{op}' with {} and {}", a.ty, b.ty),
        )),
    }
}
fn checked_int(value: Option<i64>, operator: &str, line: usize) -> Result<Value> {
    value
        .map(|value| val(Ty::Int, Data::Int(value)))
        .ok_or_else(|| Error::new(line, format!("integer overflow in '{operator}'")))
}
fn unify_runtime(
    expected: &Ty,
    actual: &Ty,
    generics: &HashSet<String>,
    substitutions: &mut HashMap<String, Ty>,
    line: usize,
) -> Result<()> {
    match (expected, actual) {
        (Ty::Named(name), actual) if generics.contains(name) => {
            if let Some(previous) = substitutions.get(name) {
                if !same(previous, actual) {
                    return Err(Error::new(
                        line,
                        format!("generic {name} was {previous}, then {actual}"),
                    ));
                }
            } else {
                substitutions.insert(name.clone(), actual.clone());
            }
            Ok(())
        }
        (Ty::List(a), Ty::List(b)) | (Ty::Arr(a), Ty::Arr(b)) => {
            unify_runtime(a, b, generics, substitutions, line)
        }
        (Ty::Map(ak, av), Ty::Map(bk, bv)) => {
            unify_runtime(ak, bk, generics, substitutions, line)?;
            unify_runtime(av, bv, generics, substitutions, line)
        }
        (Ty::Perchance(a), Ty::Perchance(b)) => unify_runtime(a, b, generics, substitutions, line),
        (Ty::Perchance(a), b) => unify_runtime(a, b, generics, substitutions, line),
        _ if accepts(expected, actual) => Ok(()),
        _ => Err(Error::new(
            line,
            format!("expected {expected}, got {actual}"),
        )),
    }
}
fn substitute_runtime(ty: &Ty, substitutions: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::Named(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Ty::Perchance(inner) => Ty::Perchance(Box::new(substitute_runtime(inner, substitutions))),
        Ty::List(inner) => Ty::List(Box::new(substitute_runtime(inner, substitutions))),
        Ty::Arr(inner) => Ty::Arr(Box::new(substitute_runtime(inner, substitutions))),
        Ty::Map(key, value) => Ty::Map(
            Box::new(substitute_runtime(key, substitutions)),
            Box::new(substitute_runtime(value, substitutions)),
        ),
        _ => ty.clone(),
    }
}
fn call(f: Value, args: &[Expr], e: EnvRef, l: usize) -> Result<Value> {
    if let Data::Builtin(builtin) = f.data.clone() {
        return call_builtin(builtin, args, e, l);
    }
    let Data::Function(mut fun) = f.data else {
        return Err(Error::new(l, "value is not callable"));
    };
    if args.len() != fun.params.len() {
        return Err(Error::new(
            l,
            format!(
                "expected {} arguments, got {}",
                fun.params.len(),
                args.len()
            ),
        ));
    }
    let mut arguments = Vec::with_capacity(args.len());
    let mut substitutions = HashMap::new();
    for ((name, expected), expression) in fun.params.iter().zip(args) {
        let value = if fun.generics.is_empty() {
            eval_expected(expression, expected, e.clone())?
        } else {
            let value = eval(expression, e.clone())?;
            unify_runtime(expected, &value.ty, &fun.generics, &mut substitutions, l)?;
            value
        };
        let actual = value.ty.clone();
        let concrete = substitute_runtime(expected, &substitutions);
        let Some(value) = conform(value, &concrete) else {
            return Err(Error::new(
                l,
                format!("argument '{name}' expects {concrete}, got {actual}"),
            ));
        };
        arguments.push(value);
    }
    if !fun.generics.is_empty() {
        fun.params = fun
            .params
            .into_iter()
            .map(|(name, ty)| (name, substitute_runtime(&ty, &substitutions)))
            .collect();
        fun.ret = substitute_runtime(&fun.ret, &substitutions);
        fun.generic_bindings = substitutions;
    }
    let name = fun.name.clone();
    let source = fun.source.clone();
    let definition_line = fun.line;
    profiler::span(
        "function",
        &name,
        source.as_deref(),
        definition_line,
        || {
            let result = call_function(fun, arguments, l);
            if result.is_err() {
                profiler::count("function_failures", 1);
            }
            result
        },
    )
}
fn call_function(fun: Function, arguments: Vec<Value>, l: usize) -> Result<Value> {
    let callenv = scope(Some(fun.closure));
    callenv.borrow_mut().type_bindings = fun.generic_bindings.clone();
    for ((name, _), value) in fun.params.iter().zip(arguments) {
        callenv.borrow_mut().values.insert(name.clone(), value);
    }
    let function_source = fun.source.clone();
    let flow = run(&fun.body, callenv).map_err(|error| {
        if let Some(source) = &function_source {
            error.with_source(source)
        } else {
            error
        }
    })?;
    match flow {
        Flow::Return(v) => {
            let v = *v;
            let actual = v.ty.clone();
            if let Some(v) = conform(v, &fun.ret) {
                Ok(v)
            } else {
                Err(Error::new(
                    l,
                    format!("function returns {actual}, declared {}", fun.ret),
                ))
            }
        }
        Flow::Normal if same(&fun.ret, &Ty::Unit) => Ok(val(Ty::Unit, Data::Unit)),
        Flow::Normal => Err(Error::new(l, format!("function must return {}", fun.ret))),
        Flow::Enough(line) => Err(Error::new(line, "'enough' can only be used inside a loop")),
        Flow::Onwards(line) => Err(Error::new(line, "'onwards' can only be used inside a loop")),
    }
}
fn call_builtin(
    builtin: Builtin,
    args: &[Expr],
    environment: EnvRef,
    line: usize,
) -> Result<Value> {
    match builtin {
        Builtin::Native { space, name, call } => {
            let arguments = args
                .iter()
                .map(|argument| eval(argument, environment.clone()))
                .collect::<Result<Vec<_>>>()?;
            let source = environment.borrow().source.clone();
            let qualified = format!("{space}.{name}");
            profiler::span("native", &qualified, source.as_deref(), line, || {
                let result = call(native::NativeCall::new(&arguments, line));
                if result.is_err() {
                    profiler::count("native_call_failures", 1);
                }
                result
            })
        }
        Builtin::NativeRuntime { space, name, call } => {
            let source = environment.borrow().source.clone();
            let qualified = format!("{space}.{name}");
            profiler::span("native", &qualified, source.as_deref(), line, || {
                let result = call(args, environment, line);
                if result.is_err() {
                    profiler::count("native_call_failures", 1);
                }
                result
            })
        }
        Builtin::Size => {
            if args.len() != 1 {
                return Err(Error::new(
                    line,
                    "size expects one string, list, array, or map",
                ));
            }
            let value = eval(&args[0], environment)?;
            profiler::span("native", "size", None, line, || {
                let size = match value.data {
                    Data::String(string) => string.chars().count(),
                    Data::List(items) | Data::Arr(items) => items.borrow().len(),
                    Data::Map(items) => items.borrow().len(),
                    _ => return Err(Error::new(line, "size expects string, list, array, or map")),
                };
                Ok(val(Ty::Int, Data::Int(size as i64)))
            })
        }
    }
}
fn execute_in(source: &str, environment: EnvRef, source_path: Option<&Path>) -> Result<()> {
    let result = (|| {
        let tokens = profiler::span("phase", "lex", source_path, 0, || lex(source))?;
        let program = profiler::span("phase", "parse", source_path, 0, || {
            Parser::new(tokens).program()
        })?;
        profiler::span("phase", "check", source_path, 0, || {
            checker::check(&program, source_path)
        })?;
        match profiler::span("phase", "execute", source_path, 0, || {
            run(&program, environment)
        })? {
            Flow::Normal | Flow::Return(_) => Ok(()),
            Flow::Enough(line) => Err(Error::new(line, "'enough' can only be used inside a loop")),
            Flow::Onwards(line) => {
                Err(Error::new(line, "'onwards' can only be used inside a loop"))
            }
        }
    })();
    result.map_err(|error| {
        if let Some(source_path) = source_path {
            error.with_source(source_path)
        } else {
            error
        }
    })
}
#[cfg(test)]
fn execute(source: &str) -> Result<()> {
    let core = root_scope();
    let environment = source_scope(core, None);
    let result = execute_in(source, environment.clone(), None);
    shutdown_native_extensions(&environment);
    result
}
fn execute_file(path: &Path) -> Result<()> {
    let canonical = profiler::span("phase", "resolve source", Some(path), 0, || {
        fs::canonicalize(path).map_err(|error| Error::new(0, error.to_string()).with_source(path))
    })?;
    let source = profiler::span("phase", "read source", Some(&canonical), 0, || {
        fs::read_to_string(&canonical)
            .map_err(|error| Error::new(0, error.to_string()).with_source(&canonical))
    })?;
    let environment = profiler::span("phase", "runtime setup", Some(&canonical), 0, || {
        let core = root_scope();
        source_scope(core, Some(canonical.clone()))
    });
    let stack = environment.borrow().load_stack.clone();
    stack.borrow_mut().push(canonical.clone());
    let result = execute_in(&source, environment.clone(), Some(&canonical));
    shutdown_native_extensions(&environment);
    stack.borrow_mut().pop();
    result
}

fn check_file(path: &Path) -> Result<()> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
    let source = fs::read_to_string(&canonical)
        .map_err(|error| Error::new(0, error.to_string()).with_source(&canonical))?;
    let program = Parser::new(lex(&source).map_err(|error| error.with_source(&canonical))?)
        .program()
        .map_err(|error| error.with_source(&canonical))?;
    checker::check(&program, Some(&canonical)).map_err(|error| error.with_source(&canonical))
}

fn trailing_program_arguments(
    mut arguments: impl Iterator<Item = String>,
) -> std::result::Result<Vec<String>, ()> {
    match arguments.next() {
        None => Ok(Vec::new()),
        Some(separator) if separator == "--" => Ok(arguments.collect()),
        Some(_) => Err(()),
    }
}

fn print_help() {
    println!(
        "Isen {}\n\nUSAGE:\n  isen <file.is> [-- <argument>...]\n  isen --check <file.is>\n  isen --diagnostics <path>...\n  isen --format [--check] <path>...\n  isen --reference [--check] [reference.md]\n  isen test [--profile <name> | <path>...]\n  isen --profile [--json <report.json>] <file.is> [-- <argument>...]\n\nOPTIONS:\n  -h, --help       Show this help\n  -V, --version    Show the Isen version",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let mut arguments = env::args().skip(1);
    let first = match arguments.next() {
        Some(p) => p,
        None => {
            eprintln!(
                "usage: isen [test [--profile <name> | <path>...] | --check <file.is> | --diagnostics <path>... | --format [--check] <path>... | --reference [--check] [reference.md] | --profile [--json <report.json>] <file.is> [-- <argument>...]]"
            );
            std::process::exit(2)
        }
    };
    if matches!(first.as_str(), "help" | "-h" | "--help") {
        if arguments.next().is_some() {
            eprintln!("usage: isen --help");
            std::process::exit(2)
        }
        print_help();
        return;
    }
    if matches!(first.as_str(), "-V" | "--version") {
        if arguments.next().is_some() {
            eprintln!("usage: isen --version");
            std::process::exit(2)
        }
        println!("isen {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if first == "--reference" {
        let mut values = arguments.collect::<Vec<_>>();
        let check = values.first().is_some_and(|value| value == "--check");
        if check {
            values.remove(0);
        }
        if values.len() > 1 {
            eprintln!("usage: isen --reference [--check] [reference.md]");
            std::process::exit(2)
        }
        let path = values
            .first()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("docs/LANGUAGE_REFERENCE.md"));
        match reference::synchronize(&path, check) {
            Ok(changed) => {
                println!(
                    "reference {} {}",
                    if check {
                        "checked"
                    } else if changed {
                        "updated"
                    } else {
                        "current"
                    },
                    path.display()
                );
                return;
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1)
            }
        }
    }
    if first == "test" || first == "--test" {
        let arguments = arguments.collect::<Vec<_>>();
        let (inputs, fail_fast) = if arguments.first().is_some_and(|value| value == "--profile") {
            if arguments.len() != 2 {
                eprintln!("usage: isen test --profile <name>");
                std::process::exit(2)
            }
            let config = match project::ProjectConfig::discover(Path::new(".")) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1)
                }
            };
            let (name, profile) = match config.test_profile(Some(&arguments[1])) {
                Ok(Some(profile)) => profile,
                Ok(None) => unreachable!(),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1)
                }
            };
            println!("using test profile {name}");
            (profile.paths.clone(), profile.fail_fast)
        } else if arguments.is_empty() {
            let config = match project::ProjectConfig::discover(Path::new(".")) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1)
                }
            };
            match config.test_profile(None) {
                Ok(Some((name, profile))) => {
                    println!("using test profile {name}");
                    (profile.paths.clone(), profile.fail_fast)
                }
                Ok(None) => (vec![PathBuf::from("tests")], false),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1)
                }
            }
        } else {
            (arguments.into_iter().map(PathBuf::from).collect(), false)
        };
        native::set_program_arguments(Vec::new()).expect("empty test arguments are valid");
        match test_runner::run_with_options(&inputs, fail_fast) {
            Ok(summary) if summary.failed == 0 => return,
            Ok(_) => std::process::exit(1),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1)
            }
        }
    }
    if first == "--diagnostics" {
        let inputs = arguments.map(PathBuf::from).collect::<Vec<_>>();
        if inputs.is_empty() {
            eprintln!("usage: isen --diagnostics <path>...");
            std::process::exit(2)
        }
        let files = match formatter::collect_files(&inputs) {
            Ok(files) => files,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1)
            }
        };
        let report = diagnostics::Report::check(&files);
        print!("{}", report.json());
        if !report.is_clean() {
            std::process::exit(1)
        }
        return;
    }
    if first == "--profile" {
        let next = arguments.next();
        let (json, path) = if next.as_deref() == Some("--json") {
            let Some(report) = arguments.next() else {
                eprintln!("usage: isen --profile --json <report.json> <file.is>");
                std::process::exit(2)
            };
            let Some(path) = arguments.next() else {
                eprintln!("usage: isen --profile --json <report.json> <file.is>");
                std::process::exit(2)
            };
            (Some(PathBuf::from(report)), path)
        } else {
            let Some(path) = next else {
                eprintln!("usage: isen --profile <file.is>");
                std::process::exit(2)
            };
            (None, path)
        };
        let program_arguments = match trailing_program_arguments(arguments) {
            Ok(arguments) => arguments,
            Err(()) => {
                eprintln!(
                    "usage: isen --profile [--json <report.json>] <file.is> [-- <argument>...]"
                );
                std::process::exit(2)
            }
        };
        if let Err(error) = native::set_program_arguments(program_arguments) {
            eprintln!("isen: {error}");
            std::process::exit(2)
        }
        let path = Path::new(&path);
        profiler::start(path);
        let result = execute_file(path);
        if result.as_ref().is_err_and(|error| !error.clean_exit) {
            profiler::count("unhandled_failures", 1);
        }
        let successful = result.is_ok() || result.as_ref().is_err_and(|error| error.clean_exit);
        let profile = profiler::finish(successful);
        eprint!("{}", profile.human());
        if let Some(json) = json {
            if let Err(error) = profile.write_json(&json) {
                eprintln!("{error}");
                std::process::exit(1)
            }
        }
        if let Err(error) = result {
            if error.clean_exit {
                return;
            }
            eprintln!("{error}");
            std::process::exit(1)
        }
        return;
    }
    if first == "--format" || first == "--fmt" {
        let next = arguments.next();
        let (checking, first_path) = if next.as_deref() == Some("--check") {
            let Some(path) = arguments.next() else {
                eprintln!("usage: isen --format --check <path>...");
                std::process::exit(2)
            };
            (true, path)
        } else {
            let Some(path) = next else {
                eprintln!("usage: isen --format <path>...");
                std::process::exit(2)
            };
            (false, path)
        };
        let inputs = std::iter::once(first_path)
            .chain(arguments)
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let files = match formatter::collect_files(&inputs) {
            Ok(files) => files,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1)
            }
        };
        if checking {
            let mut unformatted = Vec::new();
            for path in &files {
                match formatter::is_formatted(path) {
                    Ok(true) => {}
                    Ok(false) => unformatted.push(path),
                    Err(error) => {
                        eprintln!("{error}");
                        std::process::exit(1)
                    }
                }
            }
            if !unformatted.is_empty() {
                for path in unformatted {
                    eprintln!("{}: file is not formatted", path.display());
                }
                std::process::exit(1)
            }
            println!("format checked {} files", files.len());
        } else {
            // Validate every input before writing any of them. A malformed
            // file late in a directory walk must not leave an earlier prefix
            // formatted and the rest untouched.
            let mut to_format = Vec::new();
            for path in &files {
                match formatter::is_formatted(path) {
                    Ok(true) => {}
                    Ok(false) => to_format.push(path),
                    Err(error) => {
                        eprintln!("{error}");
                        std::process::exit(1)
                    }
                }
            }
            for path in &to_format {
                if let Err(error) = formatter::format_file(path) {
                    eprintln!("{error}");
                    std::process::exit(1)
                }
            }
            let changed = to_format.len();
            println!("formatted {changed} of {} files", files.len());
        }
        return;
    }
    let (checking, path) = if first == "--check" {
        let Some(path) = arguments.next() else {
            eprintln!("usage: isen --check <file.is>");
            std::process::exit(2)
        };
        (true, path)
    } else {
        (false, first)
    };
    if checking {
        if arguments.next().is_some() {
            eprintln!("usage: isen --check <file.is>");
            std::process::exit(2)
        }
    } else {
        let program_arguments = match trailing_program_arguments(arguments) {
            Ok(arguments) => arguments,
            Err(()) => {
                eprintln!("usage: isen <file.is> [-- <argument>...]");
                std::process::exit(2)
            }
        };
        if let Err(error) = native::set_program_arguments(program_arguments) {
            eprintln!("isen: {error}");
            std::process::exit(2)
        }
    }
    let result = if checking {
        check_file(Path::new(&path))
    } else {
        execute_file(Path::new(&path))
    };
    if let Err(e) = result {
        if e.clean_exit {
            return;
        }
        eprintln!("{e}");
        std::process::exit(1)
    }
    if checking {
        println!("checked {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_pre_release_collection_and_operator_surface() {
        execute(
            r#"
            borrow Array
            borrow List
            borrow Stack
            borrow Queue
            borrow Range
            borrow Map
            borrow Ordering

            form Box $ values @@ list[int], label @@ string, \$
            given add(left @@ int, right @@ int,) @@ int $ ret left + right \$
            if add(1, 2,) != 3 $ raise("trailing call comma") \$
            dec box @@ Box = Box $ values: [1, 2,], label: "old", \$
            box.values[0] += 4
            box.label = "new"
            List.append(box.values, 8)
            dec last @@ int = List.pop(box.values).pour_into(int)
            if last != 8 $ raise("bad list pop") \$
            dec growing @@ list[int] = [1, 2]
            dec visits @@ int = 0
            each value in growing $
              visits += 1
              List.append(growing, value)
            \$
            if visits != 2 $ raise("each did not use a snapshot") \$

            dec stack @@ list[int] = []
            Stack.push(stack, 3)
            if Stack.pop(stack).pour_into(int) != 3 $ raise("bad stack") \$
            dec queue @@ list[string] = []
            Queue.push(queue, "first")
            Queue.push(queue, "second")
            if Queue.pop(queue).pour_into(string) != "first" $ raise("bad queue") \$

            dec bits @@ int = 1
            bits <<= 3
            bits |= 2
            if bits != 10 || (~bits & 15) != 5 $ raise("bad bitwise") \$
            if bits & 2 == 0 $ raise("bitwise precedence") \$

            dec values @@ arr[string] = Array.sized(2, "x")
            values[1] = "y"
            dec seen @@ int = 0
            each i in Range.until(4) $ seen += i \$
            if seen != 6 $ raise("bad range") \$

            dec scores @@ map[string, int] = #{"b": 2, "a": 1,}
            dec keys @@ string = ""
            each key in scores $ keys += key \$
            if keys != "ab" $ raise("maps are not deterministic") \$
            if Map.get(scores, "missing") != naught $ raise("missing map key") \$
            if !Ordering.less("a", "b") $ raise("string ordering") \$

            given always_fails() @@ int $ raise("quietly") \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_source_level_generic_functions() {
        execute(
            r#"
            given first[T](values @@ list[T]) @@ T $
              ret values[0]
            \$
            given identity[T](value @@ T) @@ T $ ret value \$
            given through_local[T](value @@ T) @@ T $
              dec values @@ list[T] = [value]
              ret values[0]
            \$
            if first([4, 5]) != 4 $ raise("generic list") \$
            if identity("isen") != "isen" $ raise("generic value") \$
            if through_local(true) != true $ raise("generic local") \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_source_level_unit_and_implicit_unit_returns() {
        execute(
            r#"
            given announce(name @@ string) @@ unit $
              if name == "" $ raise("missing name") \$
              say("hello", name)
            \$
            dec result @@ unit = announce("Isen")
            if result != unit $ raise("unit is not the singleton value") \$

            given explicit() @@ unit $ ret unit \$
            dec second @@ unit = explicit()

            given measurement(unit @@ string) @@ string $ ret unit \$
            if measurement("ms") != "ms" $ raise("unit name did not shadow the singleton") \$
        "#,
        )
        .unwrap();

        let missing = execute("given missing() @@ int $ say(\"no result\") \\$").unwrap_err();
        assert!(missing.message.contains("can finish without ret int"));
    }

    #[test]
    fn narrows_perchance_values_by_naught_comparisons() {
        execute(
            r#"
            given accept(value @@ string) @@ unit $
              if value == "" $ raise("empty") \$
            \$

            dec present @@ perchance[string] = "Ada"
            if present != naught $
              accept(present)
              present = naught
              dec absent_after_assignment @@ naught = present
            \$ else $
              raise("wrong branch")
            \$

            dec absent @@ perchance[string] = naught
            if absent == naught $
              dec definitely_absent @@ naught = absent
            \$ else $
              accept(absent)
            \$

            dec reversed @@ perchance[string] = "Grace"
            if naught == reversed $
              raise("wrong reversed branch")
            \$ else $
              accept(reversed)
            \$

            dec looping @@ perchance[string] = "once"
            dec visits @@ int = 0
            aslongas looping != naught $
              accept(looping)
              visits += 1
              looping = naught
            \$
            if visits != 1 $ raise("while narrowing") \$
        "#,
        )
        .unwrap();

        let unnarrowed = execute(
            r#"
            given accept(value @@ string) @@ unit $ \$
            dec maybe @@ perchance[string] = "value"
            accept(maybe)
            "#,
        )
        .unwrap_err();
        assert!(unnarrowed.message.contains("expects string"));
    }

    #[test]
    fn uses_checked_integer_arithmetic() {
        let overflowing = [
            "dec value @@ int = 9223372036854775807 + 1",
            "dec value @@ int = (-9223372036854775807 - 1) - 1",
            "dec value @@ int = 9223372036854775807 * 2",
            "dec value @@ int = -(-9223372036854775807 - 1)",
            "dec value @@ int = (-9223372036854775807 - 1) / -1",
            "dec value @@ int = (-9223372036854775807 - 1) % -1",
            "dec value @@ int = 4611686018427387904 << 1",
            "dec value @@ int = 9223372036854775807\nvalue += 1",
        ];
        for source in overflowing {
            let error = execute(source).unwrap_err();
            assert!(
                error.message.contains("integer overflow"),
                "unexpected error for {source:?}: {error}"
            );
        }
    }

    #[test]
    fn evaluates_expressions_left_to_right_and_short_circuits_booleans() {
        execute(
            r#"
            dec trace @@ string = ""
            given mark(label @@ string, value @@ int) @@ int $
              trace += label
              ret value
            \$
            given three(a @@ int, b @@ int, c @@ int) @@ int $ ret a + b + c \$

            if three(mark("a", 1), mark("b", 2), mark("c", 3)) != 6 $
              raise("argument values")
            \$
            if trace != "abc" $ raise("arguments were not left-to-right") \$

            trace = ""
            if mark("l", 1) + mark("r", 2) != 3 $ raise("operator values") \$
            if trace != "lr" $ raise("operators were not left-to-right") \$

            form Pair $ first @@ int, second @@ int \$
            trace = ""
            dec pair @@ Pair = Pair $
              second: mark("s", 2),
              first: mark("f", 1)
            \$
            if pair.first != 1 || trace != "sf" $ raise("form field order") \$

            dec slots @@ arr[int] = @[1]
            given position() @@ int $ trace += "i" ret 0 \$
            given replacement() @@ int $ trace += "r" ret 2 \$
            trace = ""
            slots[position()] += replacement()
            if slots[0] != 3 || trace != "ir" $ raise("assignment order") \$

            dec captured @@ int = 1
            given mutate_and_return() @@ int $
              captured = 10
              ret 2
            \$
            captured += mutate_and_return()
            if captured != 3 $ raise("compound assignment did not capture the left value first") \$

            given dangerous() @@ bool $ raise("short circuit failed") \$
            if false && dangerous() $ raise("false and") \$
            if !(true || dangerous()) $ raise("true or") \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn uses_structural_equality_for_ordinary_values() {
        execute(
            r#"
            borrow Json
            form Person $ name @@ string, scores @@ list[int] \$
            problem Failure $ code @@ int \$

            if [1, 2] != [1, 2] $ raise("list equality") \$
            if @[1, 2] != @[1, 2] $ raise("array equality") \$
            if #{"a": 1, "b": 2} != #{"b": 2, "a": 1} $ raise("map equality") \$

            dec left @@ Person = Person $ name: "Ada", scores: [1, 2] \$
            dec right @@ Person = Person $ scores: [1, 2], name: "Ada" \$
            if left != right $ raise("form equality") \$
            right.scores[1] = 3
            if left == right $ raise("nested form inequality") \$

            dec first_failure @@ Failure = Failure $ message: "bad", code: 7 \$
            dec second_failure @@ Failure = Failure $ code: 7, message: "bad" \$
            if first_failure != second_failure $ raise("problem equality") \$

            dec first_json @@ json = Json.parse("{\"a\":[1,true]}")
            dec second_json @@ json = Json.parse("{\"a\":[1,true]}")
            if first_json != second_json $ raise("json equality") \$
            if unit != unit $ raise("unit equality") \$

            form Node $ value @@ int, next @@ perchance[Node] \$
            dec first_node @@ Node = Node $ value: 1, next: naught \$
            dec second_node @@ Node = Node $ value: 1, next: naught \$
            first_node.next = first_node
            second_node.next = second_node
            if first_node != second_node $ raise("cyclic structural equality") \$
            second_node.value = 2
            if first_node == second_node $ raise("cyclic structural inequality") \$

            given socket_is_absent(value @@ perchance[udp_socket]) @@ bool $
              ret value == naught
            \$
        "#,
        )
        .unwrap();

        let resource = execute(
            r#"
            borrow Udp
            dec handle @@ udp_socket = Udp.bind("127.0.0.1", 0)
            if handle == handle $ say("same") \$
            "#,
        )
        .unwrap_err();
        assert!(
            resource
                .message
                .contains("cannot use '==' with udp_socket and udp_socket")
        );
    }

    #[test]
    fn preserves_shared_aggregate_aliases_and_shallow_functional_copies() {
        execute(
            r#"
            borrow List
            form Person $ name @@ string \$

            dec person @@ Person = Person $ name: "Ada" \$
            dec person_alias @@ Person = person
            person_alias.name = "Grace"
            if person.name != "Grace" $ raise("form alias") \$

            given rename(value @@ Person) @@ unit $ value.name = "Lin" \$
            given same_person(value @@ Person) @@ Person $ ret value \$
            rename(person)
            dec returned @@ Person = same_person(person)
            returned.name = "Mara"
            if person.name != "Mara" $ raise("argument or return alias") \$

            dec list_value @@ list[int] = [1]
            dec list_alias @@ list[int] = list_value
            List.append(list_alias, 2)
            if size(list_value) != 2 $ raise("list alias") \$

            dec array_value @@ arr[int] = @[1]
            dec array_alias @@ arr[int] = array_value
            array_alias[0] = 2
            if array_value[0] != 2 $ raise("array alias") \$

            dec map_value @@ map[string, int] = #{"x": 1}
            dec map_alias @@ map[string, int] = map_value
            map_alias["x"] = 2
            if map_value["x"] != 2 $ raise("map alias") \$

            dec inner @@ list[int] = [1]
            dec outer @@ list[list[int]] = [inner]
            dec copied @@ list[list[int]] = List.push(outer, [2])
            copied[0][0] = 9
            List.append(copied, [3])
            if inner[0] != 9 $ raise("List.push was not shallow") \$
            if size(outer) != 1 || size(copied) != 3 $ raise("List.push shared its outer list") \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn requires_explicit_generic_parameters() {
        let undeclared = execute(
            r#"
            given identity(value @@ T) @@ T $ ret value \$
            "#,
        )
        .unwrap_err();
        assert!(undeclared.message.contains("unknown form type 'T'"));

        let typo = execute(
            r#"
            form User $ name @@ string \$
            given name(value @@ USER) @@ string $ ret value.name \$
            "#,
        )
        .unwrap_err();
        assert!(typo.message.contains("unknown form type 'USER'"));
    }

    #[test]
    fn rejects_unconstrained_generic_ordering() {
        let error = execute(
            r#"
            borrow Ordering
            given less[T](left @@ T, right @@ T) @@ bool $
              ret Ordering.less(left, right)
            \$
            "#,
        )
        .unwrap_err();
        assert!(error.message.contains("incompatible type T"));
    }

    #[test]
    fn reference_tracks_critical_language_contracts() {
        let reference = include_str!("../docs/LANGUAGE_REFERENCE.md");
        assert!(reference.contains("given first[T](values @@ list[T])"));
        assert!(!reference.contains("cannot currently be reassigned"));
        assert!(!reference.contains("otherwise-unknown all-uppercase"));
        assert!(!reference.contains("String.show(value @@ any)"));
        assert!(reference.contains("`scream`, `raise`, or `exit`"));
        assert!(reference.contains("(* BEGIN GENERATED:SOURCE_TYPES *)"));
        assert!(reference.contains("\"json\""));
        assert!(reference.contains("<!-- BEGIN GENERATED:OPERATORS -->"));
        assert!(reference.contains("borrow parse from \"json.is\" as parse_json"));
        assert!(reference.contains("### `Stack` and `Queue`"));
        assert!(reference.contains("### `Test`"));
        assert!(reference.contains("### `Bytes`, `Udp`, `Tcp`, and `Http`"));
        assert!(reference.contains("Udp.receive(arg1 @@ udp_socket"));
        assert!(reference.contains("Tcp.connect(arg1 @@ string"));
        assert!(reference.contains("Http.request(arg1 @@ string"));
        assert!(!reference.contains("Socket.bind"));
    }

    #[test]
    fn materializes_native_packages_only_when_borrowed() {
        let root = root_scope();
        assert!(root.borrow().packages.is_empty());
        assert!(root.borrow().loaded_packages.is_empty());
        let source = source_scope(root.clone(), None);
        assert!(package(&source, "Maths").is_some());
        assert!(root.borrow().loaded_packages.contains_key("Maths"));
        assert_eq!(root.borrow().loaded_packages.len(), 1);
    }

    struct TestStash {
        path: PathBuf,
    }
    impl TestStash {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!("isen-stash-{}-{unique}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
        fn write(&self, name: &str, source: &str) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, source).unwrap();
            path
        }
    }
    impl Drop for TestStash {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parses_large_examples() {
        for path in [
            "examples/gru_lm.is",
            "examples/gru_chat.is",
            "examples/showcase.is",
            "examples/library_app.is",
            "examples/lib/temperature.is",
        ] {
            let source = fs::read_to_string(path).unwrap();
            let tokens = lex(&source).unwrap();
            Parser::new(tokens).program().unwrap();
        }
    }

    #[test]
    fn borrows_shared_functions_and_forms_from_cached_stashes() {
        let directory = TestStash::new();
        directory.write(
            "library.is",
            r#"
            dec calls @@ int = 0
            form Count $ value @@ int \$
            share Count
            given next() @@ Count $
              calls = calls + 1
              ret Count $ value: calls \$
            \$
            share next
            "#,
        );
        let app = directory.write(
            "app.is",
            r#"
            borrow Count from "library.is"
            borrow next from "library.is"
            dec first @@ Count = next()
            borrow next from "library.is"
            dec second @@ Count = next()
            if first.value != 1 || second.value != 2 $ scream("stash ran twice") \$
            "#,
        );
        execute_file(&app).unwrap();
    }

    #[test]
    fn aliases_names_borrowed_from_stashes() {
        let directory = TestStash::new();
        directory.write(
            "json.is",
            "given parse(value @@ string) @@ string $ ret \"json:\" + value \\$\nshare parse",
        );
        directory.write(
            "config.is",
            "given parse(value @@ string) @@ string $ ret \"config:\" + value \\$\nshare parse",
        );
        let app = directory.write(
            "app.is",
            r#"
            borrow parse from "json.is" as parse_json
            borrow parse from "config.is" as parse_config
            if parse_json("x") != "json:x" $ raise("json alias") \$
            if parse_config("x") != "config:x" $ raise("config alias") \$
            "#,
        );
        execute_file(&app).unwrap();

        let duplicate = directory.write(
            "duplicate.is",
            r#"
            borrow parse from "json.is" as parse_value
            borrow parse from "config.is" as parse_value
            "#,
        );
        let error = execute_file(&duplicate).unwrap_err();
        assert!(error.message.contains("already defined in this scope"));

        let reserved = execute("borrow parse from \"json.is\" as if").unwrap_err();
        assert!(
            reserved
                .message
                .contains("cannot be used as a borrow alias")
        );

        let extension = execute("borrow Maths as M").unwrap_err();
        assert!(
            extension
                .message
                .contains("only names borrowed from a stash may be aliased")
        );

        directory.write("types.is", "form Shape $ sides @@ int \\$\nshare Shape");
        let type_alias =
            directory.write("type_alias.is", "borrow Shape from \"types.is\" as Polygon");
        let error = execute_file(&type_alias).unwrap_err();
        assert!(error.message.contains("cannot alias shared type 'Shape'"));
    }

    #[test]
    fn borrows_from_a_project_linked_stash() {
        let directory = TestStash::new();
        let app = directory.path.join("app");
        let shared = directory.path.join("shared");
        fs::create_dir(&app).unwrap();
        fs::create_dir(&shared).unwrap();
        fs::write(
            app.join("isen.toml"),
            "[stash_links]\nlinked = \"../shared\"\n",
        )
        .unwrap();
        fs::write(
            shared.join("answer.is"),
            "dec answer @@ int = 42\nshare answer\n",
        )
        .unwrap();
        let entry = app.join("main.is");
        fs::write(
            &entry,
            "borrow answer from \"linked/answer.is\"\ndec result @@ int = answer\n",
        )
        .unwrap();
        execute_file(&entry).unwrap();
    }

    #[test]
    fn keeps_unshared_stash_names_private() {
        let directory = TestStash::new();
        directory.write("library.is", "dec secret @@ int = 7");
        let app = directory.write("app.is", "borrow secret from \"library.is\"");
        let error = execute_file(&app).unwrap_err();
        assert!(error.message.contains("does not share 'secret'"));
    }

    #[test]
    fn shares_custom_problem_types_across_stash_boundaries() {
        let directory = TestStash::new();
        directory.write(
            "library.is",
            r#"
            problem LibraryFailure $ code @@ int \$
            share LibraryFailure
            given fail() @@ int $
              scream(LibraryFailure $ message: "library broke", code: 73 \$)
              ret 0
            \$
            share fail
            "#,
        );
        let app = directory.write(
            "app.is",
            r#"
            borrow LibraryFailure from "library.is"
            borrow fail from "library.is"
            dec recovered @@ int = 0
            attempt $
              dec ignored @@ int = fail()
            \$ recover fault @@ LibraryFailure $
              recovered = fault.code
            \$
            if recovered != 73 $ scream("shared problem was not recovered") \$
            "#,
        );
        execute_file(&app).unwrap();
    }

    #[test]
    fn rejects_circular_stash_borrowing_with_the_path_chain() {
        let directory = TestStash::new();
        directory.write(
            "a.is",
            "borrow from_b from \"b.is\"\ndec from_a @@ int = 1\nshare from_a",
        );
        directory.write(
            "b.is",
            "borrow from_a from \"a.is\"\ndec from_b @@ int = 2\nshare from_b",
        );
        let error = execute_file(&directory.path.join("a.is")).unwrap_err();
        assert!(error.message.contains("circular stash borrowing"));
        assert!(error.message.contains("a.is"));
        assert!(error.message.contains("b.is"));
    }

    #[test]
    fn reports_the_stash_source_for_borrowed_function_errors() {
        let directory = TestStash::new();
        let library = directory.write(
            "library.is",
            r#"
            given explode() @@ int $ ret missing_name \$
            share explode
            "#,
        );
        let app = directory.write(
            "app.is",
            "borrow explode from \"library.is\"\ndec result @@ int = explode()",
        );
        let error = execute_file(&app).unwrap_err();
        assert_eq!(error.source, Some(fs::canonicalize(library).unwrap()));
        assert!(error.message.contains("unknown name 'missing_name'"));
    }

    #[test]
    fn persists_float_arrays_and_text_metadata() {
        let directory =
            env::temp_dir().join(format!("isen-checkpoint-test-{}", std::process::id()));
        let directory_text = directory.to_string_lossy();
        let source = format!(
            r#"
            borrow File
            borrow Array
            File.make_dir("{directory_text}")
            File.write("{directory_text}/metadata.txt", "64\n3")
            dec metadata @@ list[string] = File.lines("{directory_text}/metadata.txt")
            if metadata[0] != "64" || metadata[1] != "3" $ ret 0 \$
            dec original @@ arr[float] = Array.float(3, 1.25)
            original[1] = -2.5
            Array.save("{directory_text}/weights.bin", original)
            dec loaded @@ arr[float] = Array.load_float("{directory_text}/weights.bin")
            if size(loaded) != 3 || loaded[0] != 1.25 || loaded[1] != -2.5 $
              ret 0
            \$
            File.write("{directory_text}/status.txt", "ok")
            "#
        );
        let result = execute(&source);
        let status = fs::read_to_string(directory.join("status.txt"));
        let _ = fs::remove_dir_all(&directory);
        result.unwrap();
        assert_eq!(status.unwrap(), "ok");
    }

    #[test]
    fn executes_typed_collections_functions_and_namespaces() {
        execute(
            r#"
            form Box $ values @@ arr[list[int]] \$
            given add(values @@ list[int]) @@ int $
                dec total @@ int = 0
                each value in values $ total = total + value \$
                ret total
            \$
            space Numbers $ given twice(value @@ int) @@ int $ ret value * 2 \$ \$
            dec item @@ Box = Box $ values: @[[1, 2], [3, 4]] \$
            dec answer @@ int = Numbers.twice(add(item.values[1]))
            dec labels @@ map[string, int] = #{ "answer": answer }
            if labels["answer"] != 14 $ ret 0 \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_mismatched_collection_elements() {
        let error = execute("dec broken = [1, true]").unwrap_err();
        assert!(error.message.contains("list contains both int and bool"));
    }

    #[test]
    fn statically_checks_untaken_paths_and_function_returns() {
        let branch = execute(
            r#"
            if false $
              dec impossible @@ string = 42
            \$
            "#,
        )
        .unwrap_err();
        assert!(branch.message.contains("expects string"));

        let call = execute(
            r#"
            given add(value @@ int) @@ int $ ret value + 1 \$
            if false $ dec impossible @@ int = add("wrong") \$
            "#,
        )
        .unwrap_err();
        assert!(call.message.contains("expects int"));

        let missing_return = execute(
            r#"
            given incomplete(flag @@ bool) @@ int $
              if flag $ ret 1 \$
            \$
            "#,
        )
        .unwrap_err();
        assert!(
            missing_return
                .message
                .contains("can finish without ret int")
        );
    }

    #[test]
    fn statically_checks_borrowed_stashes_before_running_the_entry_file() {
        let directory = TestStash::new();
        let library = directory.write(
            "broken.is",
            r#"
            given incomplete(flag @@ bool) @@ int $
              if flag $ ret 1 \$
            \$
            share incomplete
            "#,
        );
        let marker = directory.path.join("should-not-exist.txt");
        let app = directory.write(
            "app.is",
            &format!(
                "borrow File\nFile.write({:?}, \"ran\")\nborrow incomplete from \"broken.is\"",
                marker.to_string_lossy()
            ),
        );
        let error = execute_file(&app).unwrap_err();
        assert_eq!(error.source, Some(fs::canonicalize(library).unwrap()));
        assert!(error.message.contains("can finish without ret int"));
        assert!(!marker.exists());
    }

    #[test]
    fn converts_explicitly_between_scalars() {
        execute(
            r#"
            dec number @@ int = "42".pour_into(int64)
            dec label @@ string = number.pour_into(string)
            dec flag @@ bool = "true".pour_into(bool)
            if label != "42" $ ret number \$
            if !flag $ ret number \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_naught_and_enough() {
        execute(
            r#"
            dec absent @@ naught = naught
            if absent != naught $ scream("test assertion failed") \$

            dec answer @@ perchance[string] = naught
            answer = "possibly"
            if answer != "possibly" $ scream("test assertion failed") \$
            dec definite @@ string = answer.pour_into(string)
            if definite != "possibly" $ scream("test assertion failed") \$

            dec choices = ["yes", naught, "no"]
            if choices[0] != "yes" || choices[1] != naught $ scream("test assertion failed") \$

            dec options @@ map[string, perchance[int]] = #{ "one": 1, "none": naught }
            options["none"] = 2
            if options["none"] != 2 $ scream("test assertion failed") \$

            given echo(value @@ perchance[string]) @@ perchance[string] $
              ret value
            \$
            dec echoed @@ perchance[string] = echo(naught)
            if echoed != naught $ scream("test assertion failed") \$

            form Note $ body @@ perchance[string] \$
            dec empty @@ Note = Note $ body: naught \$
            dec filled @@ Note = Note $ body: "hello" \$
            if empty.body != naught || filled.body != "hello" $ scream("test assertion failed") \$

            dec visits @@ int = 0
            each outer in [1, 2, 3] $
              each inner in [1, 2, 3] $
                visits = visits + 1
                enough
              \$
            \$
            aslongas true $ enough \$
            if visits != 3 $ scream("test assertion failed") \$

            dec sum @@ int = 0
            each outer in [1, 2] $
              each inner in [1, 2, 3] $
                if inner == 2 $ onwards \$
                sum = sum + inner
              \$
            \$
            if sum != 8 $ scream("test assertion failed") \$

            dec step @@ int = 0
            dec odd_steps @@ int = 0
            aslongas step < 5 $
              step = step + 1
              if step % 2 == 0 $ onwards \$
              odd_steps = odd_steps + 1
            \$
            if odd_steps != 3 $ scream("test assertion failed") \$
            "#,
        )
        .unwrap();

        let error = execute("enough").unwrap_err();
        assert!(error.message.contains("only be used inside a loop"));

        let error = execute("onwards").unwrap_err();
        assert!(error.message.contains("only be used inside a loop"));

        let error = execute("dec impossible @@ string = naught").unwrap_err();
        assert!(error.message.contains("expects string"));

        let error = execute(
            "dec absent @@ perchance[string] = naught\ndec value = absent.pour_into(string)",
        )
        .unwrap_err();
        assert!(error.message.contains("cannot pour naught into string"));
    }

    #[test]
    fn binds_typed_declarations_inside_expressions() {
        execute(
            r#"
            dec values @@ arr[int] = @[2, 3, 0]
            dec index @@ int = 0
            dec total @@ int = 0
            aslongas (dec current @@ int = values[index]) != 0 $
              total = total + current
              index = index + 1
            \$
            if current != 0 || total != 5 $ scream("test assertion failed") \$
            "#,
        )
        .unwrap();

        let error = execute(
            r#"
            dec source @@ int = 1
            aslongas (dec current @@ bool = source) == true $ ret 0 \$
            "#,
        )
        .unwrap_err();
        assert!(error.message.contains("expects bool"));
    }

    #[test]
    fn shouts_warnings_and_screams_exceptions() {
        execute(
            r#"
            shout("this continues", 7)
            dec survived @@ bool = true
            if !survived $ scream("test assertion failed") \$
            "#,
        )
        .unwrap();

        let error = execute(
            r#"
            given fail(code @@ int) @@ int $
              scream("failure", code)
              ret 0
            \$
            dec unreachable @@ int = fail(7)
            "#,
        )
        .unwrap_err();
        assert_eq!(error.message, "SCREAMING!!! : failure 7");
        assert!(!error.clean_exit);

        let error = execute("dec colour = LengText.green(\"not stolen\")").unwrap_err();
        assert!(error.message.contains("unknown name 'LengText'"));

        let error = execute(
            r#"
            borrow LengText
            scream(LengText.green("failure"))
            "#,
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "\x1b[31mSCREAMING!!!\x1b[0m : \x1b[32mfailure\x1b[0m"
        );

        let error = execute("borrow DefinitelyNotReal").unwrap_err();
        assert!(error.message.contains("no shipped runtime space named"));

        let error = execute("borrow useful from \"package.is\"").unwrap_err();
        assert!(error.message.contains("requires a source file"));
    }

    #[test]
    fn attempts_recover_runtime_failures_and_always_clean_up() {
        execute(
            r#"
            dec recovered @@ string = ""
            dec cleaned @@ int = 0
            attempt $
              dec values @@ list[int] = []
              say(values[0])
            \$ recover problem @@ Problem $
              recovered = problem.message
            \$ always $
              cleaned = cleaned + 1
            \$
            if recovered != "list index out of bounds" || cleaned != 1 $
              scream("recovery failed")
            \$

            given answer() @@ int $
              attempt $
                ret 42
              \$ always $
                cleaned = cleaned + 1
              \$
            \$
            if answer() != 42 || cleaned != 2 $ scream("always lost a ret") \$
            "#,
        )
        .unwrap();

        let exit = execute(
            r#"
            attempt $ exit \$ recover problem @@ Problem $
              scream("exit was caught", problem.message)
            \$ always $
              dec cleanup_ran @@ bool = true
            \$
            "#,
        )
        .unwrap_err();
        assert!(exit.clean_exit);

        let error = execute(
            r#"
            attempt $ scream("original") \$ always $ scream("cleanup") \$
            "#,
        )
        .unwrap_err();
        assert_eq!(error.message, "SCREAMING!!! : cleanup");

        let static_error = execute(
            r#"
            attempt $
              dec impossible @@ int = "not an int"
            \$ recover problem @@ Problem $
              say(problem.message)
            \$
            "#,
        )
        .unwrap_err();
        assert!(static_error.message.contains("expects int"));

        execute(
            r#"
            problem MachineJammed $ gear @@ int \$
            dec recovered_gear @@ int = 0
            dec exact @@ MachineJammed = MachineJammed $
              message: "teeth locked", gear: 4
            \$
            dec general @@ Problem = exact
            given describe(fault @@ Problem) @@ string $ ret fault.message \$
            if describe(exact) != "teeth locked" $ scream("problem inheritance failed") \$
            attempt $
              scream(general)
            \$ recover fault @@ MachineJammed $
              recovered_gear = fault.gear
            \$ recover fallback @@ Problem $
              scream("wrong recovery clause", fallback.message)
            \$
            if recovered_gear != 4 $ scream("typed recovery failed") \$
            "#,
        )
        .unwrap();

        let wrong_type = execute(
            r#"
            attempt $ scream("failure") \$ recover value @@ string $ say(value) \$
            "#,
        )
        .unwrap_err();
        assert!(
            wrong_type
                .message
                .contains("recover expects a problem type")
        );

        let incomplete = execute(r"attempt $ say(1) \$").unwrap_err();
        assert!(incomplete.message.contains("requires a recover block"));
    }

    #[test]
    fn exit_exits_cleanly_from_any_depth() {
        let exit = execute(
            r#"
            given decide(done @@ bool) @@ int $
              if done $ exit \$
              ret 7
            \$
            dec unreachable @@ int = decide(true)
              scream("test assertion failed")
            "#,
        )
        .unwrap_err();
        assert!(exit.clean_exit);
        assert_eq!(exit.line, 3);

        let error = execute("exit()").unwrap_err();
        assert!(!error.clean_exit);
    }

    #[test]
    fn tokenizes_text_for_language_level_models() {
        execute(
            r#"
            borrow String
            borrow File
            dec tokens @@ list[string] = String.tokens("Birds cross the river.")
            if size(tokens) != 5 $ ret 0 \$
            dec paragraphs @@ list[string] = String.paragraph_tokens("One.\n\nTwo.")
            if size(paragraphs) != 5 $ ret 0 \$
            if paragraphs[2] != "<paragraph>" $ ret 0 \$
            dec corpora @@ list[string] = File.text_files("examples")
            if size(corpora) == 0 $ ret 0 \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_typed_empty_collections_practical_text_and_maths() {
        execute(
            r#"
            borrow List
            borrow String
            borrow Maths

            dec seen @@ list[int] = []
            seen = List.push(seen, 7)
            if size(seen) != 1 $ scream("typed list accumulation failed") \$
            seen = []
            dec nested @@ list[list[string]] = [[], ["word"]]
            dec samples @@ arr[float] = @[]
            dec scores @@ map[string, float] = #{}
            scores["Ada"] = 9.5

            if size(seen) != 0 || size(nested[0]) != 0 || size(samples) != 0 $
              scream("typed empty collection failed")
            \$
            if scores["Ada"] != 9.5 $ scream("empty map failed") \$

            dec parts @@ list[string] = String.split("one::two", "::")
            dec tokens @@ list[string] = String.tokens("R2-D2 + 7")
            if "héllo"[1] != "é" $ scream("string indexing failed") \$
            if String.slice("héllo", 1, 4) != "éll" $ scream("string slicing failed") \$
            if String.join(parts, "|") != "one|two" $ scream("string joining failed") \$
            if String.join([], ",") != "" $ scream("empty string joining failed") \$
            if String.find("héllo", "ll") != 2 $ scream("string finding failed") \$
            if tokens[0] != "R2" || tokens[1] != "-" || tokens[3] != "+" || tokens[4] != "7" $
              scream("string token preservation failed")
            \$

            if Maths.abs(-7) != 7 || Maths.abs(-2.5) != 2.5 $
              scream("absolute value failed")
            \$
            if Maths.floor(-1.2) != -2 || Maths.min(4, 9) != 4 || Maths.max(4.0, 9.0) != 9.0 $
              scream("maths helpers failed")
            \$
            if Maths.pow(2.0, 8.0) != 256.0 $ scream("power failed") \$
            "#,
        )
        .unwrap();
    }

    #[test]
    fn normalizes_text_for_corpus_models() {
        execute(
            r#"
            borrow String
            dec normalized @@ string = String.lower("Rational CHOICE")
            if normalized != "rational choice" $ ret 0 \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_seeded_randomness_and_process_arguments() {
        native::set_program_arguments(vec![
            "alpha".into(),
            "--mode=fast".into(),
            "two words".into(),
            "--verbose".into(),
        ])
        .unwrap();
        let result = execute(
            r#"
            borrow Random
            borrow Args
            borrow Kwargs
            Random.seed(42)
            dec first @@ int = Random.int(-1000, 1000)
            dec second @@ float = Random.float(0.0, 1.0)
            Random.seed(42)
            if Random.int(-1000, 1000) != first $ scream("seed did not repeat") \$
            if Random.float(0.0, 1.0) != second $ scream("seed did not repeat") \$
            dec arguments @@ list[string] = Args.all()
            if size(arguments) != 2 || arguments[0] != "alpha" || Args.get(1) != "two words" $
              scream("arguments were not passed through")
            \$
            if Args.get(-1) != naught || Args.get(2) != naught $ scream("argument bounds failed") \$
            dec keywords @@ map[string, string] = Kwargs.all()
            if size(keywords) != 2 || keywords["mode"] != "fast" || keywords["verbose"] != "true" $
              scream("keyword arguments were not parsed")
            \$
            if !Kwargs.has("mode") || Kwargs.get("missing") != naught $
              scream("keyword lookup failed")
            \$
            "#,
        );
        native::set_program_arguments(Vec::new()).unwrap();
        result.unwrap();
    }

    #[test]
    fn supports_environment_and_path_inspection() {
        let directory = TestStash::new();
        let file = directory.write("sample.txt", "hello");
        let canonical_file = fs::canonicalize(&file).unwrap();
        let directory_text = directory.path.to_string_lossy();
        let file_text = file.to_string_lossy();
        let canonical_file_text = canonical_file.to_string_lossy();
        execute(&format!(
            r#"
            borrow Env
            borrow Path
            if Env.get("__ISEN_TEST_VARIABLE_THAT_DOES_NOT_EXIST__") != naught $
              scream("missing environment value was present")
            \$
            if !Path.exists({file_text:?}) || !Path.is_file({file_text:?}) $
              scream("file inspection failed")
            \$
            if !Path.is_dir({directory_text:?}) || Path.name({file_text:?}) != "sample.txt" $
              scream("directory inspection failed")
            \$
            if Path.join({directory_text:?}, "sample.txt") != {file_text:?} $
              scream("path joining failed")
            \$
            dec entries @@ list[string] = Path.list({directory_text:?})
            if size(entries) != 1 || entries[0] != {file_text:?} $ scream("path listing failed") \$
            if Path.canonical({file_text:?}) != {canonical_file_text:?} $ scream("canonical path failed") \$
            "#,
        ))
        .unwrap();
    }

    #[test]
    fn supports_float_math_and_mutable_float_arrays() {
        execute(
            r#"
            borrow Array
            borrow Maths
            borrow Random
            dec weights @@ arr[float] = Array.float(2, 0.0)
            weights[1] = Maths.exp(1.0)
            dec random @@ float = Random.float(-1.0, 1.0)
            if weights[1] <= 1.0 $ ret 0 \$
            if random < -1.0 || random > 1.0 $ ret 0 \$
            if Maths.sin(Maths.pi / 2.0) < 0.999 $ scream("bad sine") \$
            if Maths.cos(Maths.pi) > -0.999 $ scream("bad cosine") \$
            if Maths.tau < 6.28 || Maths.e < 2.71 $ scream("bad constants") \$
            if Maths.phi < 1.61 || Maths.sqrt_two < 1.41 || Maths.ln_two < 0.69 $
              scream("bad secondary constants")
            \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_mutable_word_id_maps() {
        execute(
            r#"
            borrow Map
            dec ids @@ map[string, int] = Map.string_int()
            ids["harbor"] = 7
            if !Map.has(ids, "harbor") $ ret 0 \$
            if ids["harbor"] != 7 $ ret 0 \$
        "#,
        )
        .unwrap();
    }

    #[test]
    fn supports_ranked_maps() {
        execute(
            r#"
            borrow Map
            dec counts @@ map[string, int] = Map.string_int()
            counts["rare"] = 1
            counts["common"] = 3
            dec top @@ list[string] = Map.top_string_int(counts, 1)
            if top[0] != "common" $ raise("map ranking failed") \$
        "#,
        )
        .unwrap();
    }

    #[cfg(feature = "ml-kernels")]
    #[test]
    fn supports_ranked_maps_and_fused_mlp_kernels() {
        execute(
            r#"
            borrow Array
            borrow ML
            borrow Map
            dec counts @@ map[string, int] = Map.string_int()
            counts["rare"] = 1
            counts["common"] = 3
            dec top @@ list[string] = Map.top_string_int(counts, 1)
            if top[0] != "common" $ ret 0 \$

            dec ids @@ arr[int] = Array.int(3, 7)
            if ids[2] != 7 $ ret 0 \$
            dec hidden @@ arr[float] = Array.float(2, 0.0)
            dec bias @@ arr[float] = Array.float(2, 0.0)
            dec matrix @@ arr[float] = Array.float(4, 0.1)
            dec embedding @@ arr[float] = Array.float(6, 0.2)
            dec matrix_two @@ arr[float] = Array.float(4, 0.1)
            dec embedding_two @@ arr[float] = Array.float(6, 0.2)
            dec matrix_three @@ arr[float] = Array.float(4, 0.1)
            dec embedding_three @@ arr[float] = Array.float(6, 0.2)
            dec output @@ arr[float] = Array.float(6, 0.1)
            dec output_bias @@ arr[float] = Array.float(3, 0.0)
            dec gradient @@ arr[float] = Array.float(2, 0.0)
            ML.mlp_forward(hidden, bias, matrix, embedding, 0,
              matrix_two, embedding_two, 1,
              matrix_three, embedding_three, 2, 2)
            dec loss @@ float = ML.sampled_update(output, output_bias,
              hidden, gradient, 1, 1.0, 0.1, 2)
            Array.fill(gradient, 0, 2, 0.0)
            dec samples @@ arr[int] = Array.int(3, 0)
            samples[0] = 1
            samples[1] = 0
            samples[2] = 2
            dec softmax_loss @@ float = ML.sampled_softmax_update(output,
              output_bias, hidden, gradient, samples, 3, 0.1, 2)
            ML.mlp_backprop(embedding, matrix, embedding_two, matrix_two,
              embedding_three, matrix_three, bias, hidden, gradient,
              0, 1, 2, 2, 0.1)
            dec picked @@ int = ML.softmax_sample(output, output_bias,
              hidden, 3, 2, 1.0, 1, 1.2)
            if loss < 0.0 || softmax_loss <= 0.0 || picked < 1 || picked > 2 $
              ret 0
            \$
        "#,
        )
        .unwrap();
    }

    #[cfg(feature = "ml-kernels")]
    #[test]
    fn supports_fused_gru_kernels() {
        execute(
            r#"
            borrow Array
            borrow ML
            dec state @@ arr[float] = Array.float(2, 0.0)
            dec previous @@ arr[float] = Array.float(2, 0.0)
            dec update @@ arr[float] = Array.float(2, 0.0)
            dec reset @@ arr[float] = Array.float(2, 0.0)
            dec proposed @@ arr[float] = Array.float(2, 0.0)
            dec embedding @@ arr[float] = Array.float(6, 0.2)
            dec wz @@ arr[float] = Array.float(4, 0.1)
            dec uz @@ arr[float] = Array.float(4, 0.1)
            dec bz @@ arr[float] = Array.float(2, 0.0)
            dec wr @@ arr[float] = Array.float(4, 0.1)
            dec ur @@ arr[float] = Array.float(4, 0.1)
            dec br @@ arr[float] = Array.float(2, 0.0)
            dec wn @@ arr[float] = Array.float(4, 0.1)
            dec un @@ arr[float] = Array.float(4, 0.1)
            dec bn @@ arr[float] = Array.float(2, 0.0)
            dec gradient @@ arr[float] = Array.float(2, 0.05)
            dec recurrent @@ arr[float] = Array.float(2, 0.0)
            ML.gru_forward(state, previous, update, reset, proposed,
              embedding, 1, wz, uz, bz, wr, ur, br, wn, un, bn, 2)
            ML.gru_backprop(embedding, 1, previous, update, reset,
              proposed, gradient, recurrent, wz, uz, bz, wr, ur, br, wn, un, bn,
              2, 0.01)
            dec cached @@ arr[float] = Array.float(4, 0.0)
            Array.copy(cached, 2, state, 0, 2)
            if state[0] == 0.0 || recurrent[0] == 0.0 $ ret 0 \$
            if cached[2] != state[0] $ ret 0 \$
        "#,
        )
        .unwrap();
    }
}
