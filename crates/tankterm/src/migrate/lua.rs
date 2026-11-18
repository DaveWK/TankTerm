//! A static reader for the Lua that WezTerm configurations are written in.
//!
//! Nothing is run. Literals, tables, names and calls are read as they are written; anything
//! computed, like operators, functions and conditions, is [`Expr::Unknown`] or a
//! [`Stmt::Other`] with its line, so it can be reported as skipped.

/// A Lua expression, as far as it can be read without running it.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Nil,
    Bool(bool),
    Number(f64),
    Str(String),
    Table(Vec<(Field, Expr)>),
    /// A name or a dotted path, like `wezterm.action.Copy`.
    Name(Vec<String>),
    /// A call of a name or a dotted path, like `wezterm.font('X')`.
    Call(Vec<String>, Vec<Expr>),
    /// Something computed.
    Unknown,
}

/// The key of a table field.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// `{ 'a', 'b' }`
    Positional,
    /// `{ name = … }`
    Name(String),
    /// `{ [expr] = … }`
    Index(Expr),
}

impl Expr {
    /// The value of the field `name` of a table.
    pub fn get(&self, name: &str) -> Option<&Expr> {
        match self {
            Expr::Table(fields) => fields
                .iter()
                .rev()
                .find(|(field, _)| matches!(field, Field::Name(field) if field == name))
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The positional values of a table.
    pub fn items(&self) -> Vec<&Expr> {
        match self {
            Expr::Table(fields) => fields
                .iter()
                .filter(|(field, _)| *field == Field::Positional)
                .map(|(_, value)| value)
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Expr::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Expr::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Expr::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// A statement at the top level of the file.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `local name = value`
    Local { name: String, value: Expr, line: usize },
    /// `a.b.c = value`
    Assign { target: Vec<String>, value: Expr, line: usize },
    /// `table.insert(a.b, value)`
    Insert { target: Vec<String>, value: Expr, line: usize },
    /// `return value`
    Return { value: Expr, line: usize },
    /// A statement that isn't read, like an `if` or a function.
    Other { what: String, line: usize },
}

/// Read the top-level statements of a Lua file.
pub fn parse(source: &str) -> Result<Vec<Stmt>, String> {
    let tokens = tokenize(source)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut statements = Vec::new();
    while !parser.at_end() {
        statements.extend(parser.statement()?);
    }
    Ok(statements)
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Name(String),
    Str(String),
    Num(f64),
    Sym(&'static str),
}

const SYMBOLS: &[&str] = &[
    "...", "..", "==", "~=", "<=", ">=", "//", "::", "<<", ">>", "+", "-", "*", "/", "%", "^", "#",
    "&", "~", "|", "<", ">", "=", "(", ")", "{", "}", "[", "]", ";", ":", ",", ".",
];

const BINARY: &[&str] = &[
    "+", "-", "*", "/", "//", "%", "^", "..", "==", "~=", "<", "<=", ">", ">=", "&", "|", "~",
    "<<", ">>",
];

fn tokenize(source: &str) -> Result<Vec<(Tok, usize)>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1;

    // The level of a long bracket `[==[` at `i`, if there is one.
    let long_bracket = |i: usize| -> Option<usize> {
        if chars.get(i) != Some(&'[') {
            return None;
        }
        let level = chars[i + 1..].iter().take_while(|c| **c == '=').count();
        (chars.get(i + 1 + level) == Some(&'[')).then_some(level)
    };
    // Read a long string from `i` at `level`; returns its text and the index after it.
    let read_long = |i: usize, level: usize, line: &mut usize| -> Result<(String, usize), String> {
        let close: String = format!("]{}]", "=".repeat(level));
        let start = i + level + 2;
        let rest: String = chars[start..].iter().collect();
        let end = rest.find(&close).ok_or(format!("line {line}: unfinished long string"))?;
        let text: String = rest[..end].to_owned();
        *line += text.matches('\n').count();
        let text = text.strip_prefix('\n').unwrap_or(&text).to_owned();
        Ok((text, start + rest[..end].chars().count() + close.len()))
    };

    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            match long_bracket(i + 2) {
                Some(level) => i = read_long(i + 2, level, &mut line)?.1,
                None => {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                },
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push((Tok::Name(chars[start..i].iter().collect()), line));
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            let hex = c == '0' && matches!(chars.get(i + 1), Some('x' | 'X'));
            if hex {
                i += 2;
            }
            while i < chars.len() {
                let c = chars[i];
                let exponent = if hex { matches!(c, 'p' | 'P') } else { matches!(c, 'e' | 'E') };
                if exponent && matches!(chars.get(i + 1), Some('+' | '-')) {
                    i += 2;
                } else if c.is_ascii_alphanumeric() || c == '.' {
                    i += 1;
                } else {
                    break;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let number = if hex {
                i64::from_str_radix(&text[2..], 16).map(|n| n as f64).ok()
            } else {
                text.parse::<f64>().ok()
            };
            let number = number.ok_or(format!("line {line}: can't read the number {text}"))?;
            tokens.push((Tok::Num(number), line));
        } else if c == '"' || c == '\'' {
            let quote = c;
            let start_line = line;
            let mut text = String::new();
            i += 1;
            loop {
                let Some(&c) = chars.get(i) else {
                    return Err(format!("line {start_line}: unfinished string"));
                };
                i += 1;
                if c == '\\' {
                    let Some(&e) = chars.get(i) else { break };
                    i += 1;
                    match e {
                        'n' => text.push('\n'),
                        't' => text.push('\t'),
                        'r' => text.push('\r'),
                        'a' => text.push('\x07'),
                        'b' => text.push('\x08'),
                        'f' => text.push('\x0c'),
                        'v' => text.push('\x0b'),
                        'e' => text.push('\x1b'),
                        'x' => {
                            let hex: String = chars[i..chars.len().min(i + 2)].iter().collect();
                            i += hex.len();
                            text.extend(u8::from_str_radix(&hex, 16).ok().map(char::from));
                        },
                        'u' => {
                            let rest: String = chars[i..].iter().collect();
                            if let Some(end) = rest.find('}') {
                                let code = rest[..end].trim_start_matches('{');
                                text.extend(
                                    u32::from_str_radix(code, 16).ok().and_then(char::from_u32),
                                );
                                i += rest[..=end].chars().count();
                            }
                        },
                        'z' => {
                            while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                                if chars[i] == '\n' {
                                    line += 1;
                                }
                                i += 1;
                            }
                        },
                        '\n' => {
                            line += 1;
                            text.push('\n');
                        },
                        d if d.is_ascii_digit() => {
                            let mut digits = d.to_string();
                            while digits.len() < 3 && chars.get(i).is_some_and(char::is_ascii_digit)
                            {
                                digits.push(chars[i]);
                                i += 1;
                            }
                            text.extend(digits.parse::<u8>().ok().map(char::from));
                        },
                        other => text.push(other),
                    }
                } else if c == '\n' {
                    return Err(format!("line {start_line}: unfinished string"));
                } else if c == quote {
                    break;
                } else {
                    text.push(c);
                }
            }
            tokens.push((Tok::Str(text), start_line));
        } else if let Some(level) = long_bracket(i) {
            let start_line = line;
            let (text, end) = read_long(i, level, &mut line)?;
            tokens.push((Tok::Str(text), start_line));
            i = end;
        } else {
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            let symbol = SYMBOLS
                .iter()
                .find(|symbol| rest.starts_with(**symbol))
                .ok_or(format!("line {line}: unexpected {c:?}"))?;
            tokens.push((Tok::Sym(symbol), line));
            i += symbol.len();
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<(Tok, usize)>,
    pos: usize,
}

impl Parser {
    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|(tok, _)| tok)
    }

    fn line(&self) -> usize {
        self.tokens.get(self.pos).or(self.tokens.last()).map(|(_, line)| *line).unwrap_or(1)
    }

    fn next(&mut self) -> Option<Tok> {
        let tok = self.tokens.get(self.pos).map(|(tok, _)| tok.clone());
        self.pos += 1;
        tok
    }

    fn is_sym(&self, symbol: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(s)) if *s == symbol)
    }

    fn is_name(&self, name: &str) -> bool {
        matches!(self.peek(), Some(Tok::Name(n)) if n == name)
    }

    fn expect_sym(&mut self, symbol: &str) -> Result<(), String> {
        if self.is_sym(symbol) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("line {}: expected {symbol:?}", self.line()))
        }
    }

    fn name(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Tok::Name(name)) => Ok(name),
            _ => Err(format!("line {}: expected a name", self.line())),
        }
    }

    fn statement(&mut self) -> Result<Vec<Stmt>, String> {
        let line = self.line();
        let other = |what: &str| Stmt::Other { what: what.to_owned(), line };

        if self.is_sym(";") {
            self.pos += 1;
            return Ok(Vec::new());
        }
        if self.is_sym("::") {
            self.pos += 1;
            self.name()?;
            self.expect_sym("::")?;
            return Ok(vec![other("a label")]);
        }

        let keyword = match self.peek() {
            Some(Tok::Name(name)) => name.clone(),
            _ => String::new(),
        };
        match keyword.as_str() {
            "local" => {
                self.pos += 1;
                if self.is_name("function") {
                    self.skip_block()?;
                    return Ok(vec![other("a function")]);
                }
                let mut names = vec![self.name()?];
                self.attribute()?;
                while self.is_sym(",") {
                    self.pos += 1;
                    names.push(self.name()?);
                    self.attribute()?;
                }
                let values = if self.is_sym("=") {
                    self.pos += 1;
                    self.expr_list()?
                } else {
                    Vec::new()
                };
                Ok(names
                    .into_iter()
                    .enumerate()
                    .map(|(i, name)| Stmt::Local {
                        name,
                        value: values.get(i).cloned().unwrap_or(Expr::Nil),
                        line,
                    })
                    .collect())
            },
            "function" => {
                self.skip_block()?;
                Ok(vec![other("a function")])
            },
            "if" | "for" | "while" | "do" | "repeat" => {
                self.skip_block()?;
                Ok(vec![other(&format!("an `{keyword}` block"))])
            },
            "return" => {
                self.pos += 1;
                let ends = self.at_end() || self.is_name("end") || self.is_sym(";");
                let value = if ends {
                    Expr::Nil
                } else {
                    self.expr_list()?.into_iter().next().unwrap_or(Expr::Nil)
                };
                Ok(vec![Stmt::Return { value, line }])
            },
            "goto" | "break" => {
                self.pos += 1;
                if keyword == "goto" {
                    self.name()?;
                }
                Ok(vec![other(&format!("a `{keyword}`"))])
            },
            _ => {
                let target = self.suffixed()?;
                if self.is_sym("=") || self.is_sym(",") {
                    let mut targets = vec![target];
                    while self.is_sym(",") {
                        self.pos += 1;
                        targets.push(self.suffixed()?);
                    }
                    self.expect_sym("=")?;
                    let values = self.expr_list()?;
                    return Ok(targets
                        .into_iter()
                        .enumerate()
                        .map(|(i, target)| match target {
                            Expr::Name(path) => Stmt::Assign {
                                target: path,
                                value: values.get(i).cloned().unwrap_or(Expr::Nil),
                                line,
                            },
                            _ => other("an assignment to a computed place"),
                        })
                        .collect());
                }
                match target {
                    Expr::Call(path, mut args)
                        if path == ["table", "insert"] && args.len() == 2 =>
                    {
                        let value = args.pop().unwrap_or(Expr::Nil);
                        match args.pop() {
                            Some(Expr::Name(target)) => {
                                Ok(vec![Stmt::Insert { target, value, line }])
                            },
                            _ => Ok(vec![other("a table.insert into a computed place")]),
                        }
                    },
                    Expr::Call(path, _) => {
                        Ok(vec![other(&format!("a call of {}", path.join(".")))])
                    },
                    _ => Ok(vec![other("a computed statement")]),
                }
            },
        }
    }

    /// Skip a `<const>` or `<close>` attribute.
    fn attribute(&mut self) -> Result<(), String> {
        if self.is_sym("<") {
            self.pos += 1;
            self.name()?;
            self.expect_sym(">")?;
        }
        Ok(())
    }

    /// Skip a block that starts here, up to its `end` or `until` and condition.
    fn skip_block(&mut self) -> Result<(), String> {
        let line = self.line();
        let mut depth = 0;
        loop {
            match self.next() {
                Some(Tok::Name(name)) => match name.as_str() {
                    "function" | "if" | "do" | "repeat" => depth += 1,
                    "end" => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(());
                        }
                    },
                    "until" => {
                        depth -= 1;
                        if depth == 0 {
                            self.expr()?;
                            return Ok(());
                        }
                    },
                    _ => (),
                },
                Some(_) => (),
                None => return Err(format!("line {line}: block without `end`")),
            }
        }
    }

    fn expr_list(&mut self) -> Result<Vec<Expr>, String> {
        let mut exprs = vec![self.expr()?];
        while self.is_sym(",") {
            self.pos += 1;
            exprs.push(self.expr()?);
        }
        Ok(exprs)
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let mut expr = self.unary()?;
        loop {
            let binary = match self.peek() {
                Some(Tok::Sym(symbol)) => BINARY.contains(symbol),
                Some(Tok::Name(name)) => name == "and" || name == "or",
                _ => false,
            };
            if !binary {
                return Ok(expr);
            }
            self.pos += 1;
            self.unary()?;
            expr = Expr::Unknown;
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        let negative = self.is_sym("-");
        if negative || self.is_sym("#") || self.is_sym("~") || self.is_name("not") {
            self.pos += 1;
            let operand = self.unary()?;
            return Ok(match operand {
                Expr::Number(n) if negative => Expr::Number(-n),
                _ => Expr::Unknown,
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(Expr::Number(n))
            },
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(Expr::Str(s))
            },
            Some(Tok::Name(name)) if name == "nil" => {
                self.pos += 1;
                Ok(Expr::Nil)
            },
            Some(Tok::Name(name)) if name == "true" || name == "false" => {
                self.pos += 1;
                Ok(Expr::Bool(name == "true"))
            },
            Some(Tok::Name(name)) if name == "function" => {
                self.skip_block()?;
                Ok(Expr::Unknown)
            },
            Some(Tok::Sym("...")) => {
                self.pos += 1;
                Ok(Expr::Unknown)
            },
            Some(Tok::Sym("{")) => self.table(),
            _ => self.suffixed(),
        }
    }

    fn table(&mut self) -> Result<Expr, String> {
        self.expect_sym("{")?;
        let mut fields = Vec::new();
        while !self.is_sym("}") {
            if self.at_end() {
                return Err(format!("line {}: table without `}}`", self.line()));
            }
            let field = if self.is_sym("[") {
                self.pos += 1;
                let key = self.expr()?;
                self.expect_sym("]")?;
                self.expect_sym("=")?;
                Field::Index(key)
            } else if matches!(self.peek(), Some(Tok::Name(_)))
                && matches!(self.tokens.get(self.pos + 1), Some((Tok::Sym("="), _)))
            {
                let name = self.name()?;
                self.pos += 1;
                Field::Name(name)
            } else {
                Field::Positional
            };
            fields.push((field, self.expr()?));
            if self.is_sym(",") || self.is_sym(";") {
                self.pos += 1;
            } else if !self.is_sym("}") {
                return Err(format!("line {}: expected `,` or `}}` in a table", self.line()));
            }
        }
        self.pos += 1;
        Ok(Expr::Table(fields))
    }

    /// A name or parenthesized expression, with its fields, indexes and calls.
    fn suffixed(&mut self) -> Result<Expr, String> {
        let mut expr = if self.is_sym("(") {
            self.pos += 1;
            let inner = self.expr()?;
            self.expect_sym(")")?;
            inner
        } else {
            Expr::Name(vec![self.name()?])
        };

        loop {
            if self.is_sym(".") {
                self.pos += 1;
                let field = self.name()?;
                expr = match expr {
                    Expr::Name(mut path) => {
                        path.push(field);
                        Expr::Name(path)
                    },
                    _ => Expr::Unknown,
                };
            } else if self.is_sym("[") {
                self.pos += 1;
                let index = self.expr()?;
                self.expect_sym("]")?;
                expr = match (expr, index) {
                    (Expr::Name(mut path), Expr::Str(field)) => {
                        path.push(field);
                        Expr::Name(path)
                    },
                    _ => Expr::Unknown,
                };
            } else if self.is_sym(":") {
                self.pos += 1;
                self.name()?;
                self.call_args()?;
                expr = Expr::Unknown;
            } else if self.is_sym("(")
                || self.is_sym("{")
                || matches!(self.peek(), Some(Tok::Str(_)))
            {
                let args = self.call_args()?;
                expr = match expr {
                    Expr::Name(path) => Expr::Call(path, args),
                    _ => Expr::Unknown,
                };
            } else {
                return Ok(expr);
            }
        }
    }

    fn call_args(&mut self) -> Result<Vec<Expr>, String> {
        match self.peek().cloned() {
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(vec![Expr::Str(s)])
            },
            Some(Tok::Sym("{")) => Ok(vec![self.table()?]),
            Some(Tok::Sym("(")) => {
                self.pos += 1;
                if self.is_sym(")") {
                    self.pos += 1;
                    return Ok(Vec::new());
                }
                let args = self.expr_list()?;
                self.expect_sym(")")?;
                Ok(args)
            },
            _ => Err(format!("line {}: expected call arguments", self.line())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(path: &str) -> Expr {
        Expr::Name(path.split('.').map(ToOwned::to_owned).collect())
    }

    #[test]
    fn statements() {
        let source = r##"
-- A comment.
--[[ A long
comment. ]]
local wezterm = require 'wezterm'
local act <const> = wezterm.action
local config = wezterm.config_builder()

config.font = wezterm.font("Fira Code", { weight = "Bold" })
config.font_size = 12.5 * 2
config.color_scheme = [[Dracula]]
config.colors = { ansi = { '#000', "#f00" }, [ "indexed" ] = { [136] = 'x' } }
if wezterm.target_triple:find('darwin') then
  config.font_size = 14
end
wezterm.on('update-status', function(window) window:set_right_status('x') end)
table.insert(config.keys, { key = 'a', mods = 'CTRL', action = act.SpawnTab 'CurrentPaneDomain' })
config.x = -0x10
return config
"##;
        let statements = parse(source).unwrap();

        assert_eq!(statements[0], Stmt::Local {
            name: "wezterm".into(),
            value: Expr::Call(vec!["require".into()], vec![Expr::Str("wezterm".into())]),
            line: 5
        });
        assert_eq!(statements[1], Stmt::Local {
            name: "act".into(),
            value: name("wezterm.action"),
            line: 6
        });
        assert!(
            matches!(&statements[3], Stmt::Assign { target, value: Expr::Call(path, args), line: 9 }
            if target == &["config", "font"] && path == &["wezterm", "font"] && args.len() == 2)
        );
        assert!(matches!(&statements[4], Stmt::Assign { value: Expr::Unknown, line: 10, .. }));
        assert!(
            matches!(&statements[5], Stmt::Assign { value: Expr::Str(s), .. } if s == "Dracula")
        );
        let Stmt::Assign { value: colors, .. } = &statements[6] else { panic!() };
        assert_eq!(colors.get("ansi").unwrap().items().len(), 2);
        assert!(matches!(&statements[7], Stmt::Other { line: 13, .. }));
        assert!(matches!(&statements[8], Stmt::Other { line: 16, .. }));
        assert!(
            matches!(&statements[9], Stmt::Insert { target, line: 17, .. } if target == &["config", "keys"])
        );
        assert!(
            matches!(&statements[10], Stmt::Assign { value: Expr::Number(n), .. } if *n == -16.)
        );
        assert!(matches!(&statements[11], Stmt::Return { value, .. } if *value == name("config")));
    }

    #[test]
    fn strings() {
        let statements = parse(r#"x = "a\"b\\c\n\x41\u{263A}\65""#).unwrap();
        assert!(
            matches!(&statements[0], Stmt::Assign { value: Expr::Str(s), .. } if s == "a\"b\\c\nA\u{263A}A")
        );

        let statements = parse("x = 'it\"s' y = [==[a]]b]==]").unwrap();
        assert!(matches!(&statements[0], Stmt::Assign { value: Expr::Str(s), .. } if s == "it\"s"));
        assert!(matches!(&statements[1], Stmt::Assign { value: Expr::Str(s), .. } if s == "a]]b"));
    }

    #[test]
    fn errors_have_lines() {
        let err = parse("local x = {\n 'a',\n").unwrap_err();
        assert!(err.starts_with("line "), "{err}");
    }
}
