// SPDX-License-Identifier: GPL-3.0-or-later
//! Reader for legacy Lightroom `.lrtemplate` presets.
//!
//! These files are a Lua table literal assigned to a global (`s = { ... }`).
//! They come from untrusted sources (preset packs), so instead of embedding a
//! Lua VM this is a small recursive-descent parser for the data subset Adobe
//! writes: tables, strings, numbers, booleans, `nil`, comments, and the
//! localisation wrappers `LOC "..."` / `ZSTR "..."`.

use std::collections::BTreeMap;

use crate::Error;
use crate::value::Value;

/// Maximum table nesting accepted, to bound recursion on hostile input.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Sym(char),
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src: src.as_bytes(),
            pos: 0,
            line: 1,
        }
    }

    fn err(&self, msg: impl Into<String>) -> Error {
        Error::Lua {
            line: self.line,
            msg: msg.into(),
        }
    }

    fn peek_byte(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }

    fn skip_trivia(&mut self) -> Result<(), Error> {
        loop {
            match self.peek_byte(0) {
                Some(b'\n') => {
                    self.line += 1;
                    self.pos += 1;
                }
                Some(c) if c.is_ascii_whitespace() => self.pos += 1,
                Some(b'-') if self.peek_byte(1) == Some(b'-') => {
                    self.pos += 2;
                    if let Some(level) = self.long_bracket_level() {
                        self.read_long_bracket(level)?;
                    } else {
                        while let Some(c) = self.peek_byte(0) {
                            if c == b'\n' {
                                break;
                            }
                            self.pos += 1;
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// If positioned at `[[` or `[==[`, return the number of `=` signs.
    fn long_bracket_level(&self) -> Option<usize> {
        if self.peek_byte(0) != Some(b'[') {
            return None;
        }
        let mut level = 0;
        while self.peek_byte(1 + level) == Some(b'=') {
            level += 1;
        }
        (self.peek_byte(1 + level) == Some(b'[')).then_some(level)
    }

    fn read_long_bracket(&mut self, level: usize) -> Result<String, Error> {
        self.pos += level + 2;
        let close: Vec<u8> = std::iter::once(b']')
            .chain(std::iter::repeat_n(b'=', level))
            .chain(std::iter::once(b']'))
            .collect();
        let start = self.pos;
        while self.pos < self.src.len() {
            if self.src[self.pos..].starts_with(&close) {
                let text = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                self.pos += close.len();
                return Ok(text);
            }
            if self.src[self.pos] == b'\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        Err(self.err("unterminated long bracket"))
    }

    fn read_string(&mut self, quote: u8) -> Result<String, Error> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let c = self
                .peek_byte(0)
                .ok_or_else(|| self.err("unterminated string"))?;
            self.pos += 1;
            match c {
                c if c == quote => break,
                b'\n' => return Err(self.err("newline in string")),
                b'\\' => {
                    let e = self.peek_byte(0).ok_or_else(|| self.err("bad escape"))?;
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'\\' | b'"' | b'\'' => out.push(e),
                        b'\n' => {
                            self.line += 1;
                            out.push(b'\n');
                        }
                        b'0'..=b'9' => {
                            let mut n = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.peek_byte(0) {
                                    Some(d @ b'0'..=b'9') => {
                                        n = n * 10 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(u8::try_from(n).map_err(|_| self.err("escape out of range"))?);
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn read_number(&mut self) -> Result<f64, Error> {
        let start = self.pos;
        if self.peek_byte(0) == Some(b'0') && matches!(self.peek_byte(1), Some(b'x' | b'X')) {
            self.pos += 2;
            while matches!(self.peek_byte(0), Some(c) if c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            let digits = std::str::from_utf8(&self.src[start + 2..self.pos]).unwrap_or_default();
            return u64::from_str_radix(digits, 16)
                .map(|n| n as f64)
                .map_err(|_| self.err("bad hex number"));
        }
        while let Some(c) = self.peek_byte(0) {
            let exponent_sign = matches!(c, b'+' | b'-')
                && matches!(self.src.get(self.pos.wrapping_sub(1)), Some(b'e' | b'E'));
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || exponent_sign {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| self.err("bad number"))
    }

    fn next(&mut self) -> Result<Option<Token>, Error> {
        self.skip_trivia()?;
        let Some(c) = self.peek_byte(0) else {
            return Ok(None);
        };
        let token = match c {
            b'"' | b'\'' => Token::Str(self.read_string(c)?),
            b'[' if self.long_bracket_level().is_some() => {
                let level = self.long_bracket_level().unwrap_or(0);
                Token::Str(self.read_long_bracket(level)?)
            }
            b'0'..=b'9' => Token::Num(self.read_number()?),
            b'.' if matches!(self.peek_byte(1), Some(b'0'..=b'9')) => {
                Token::Num(self.read_number()?)
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = self.pos;
                while matches!(self.peek_byte(0), Some(c) if c.is_ascii_alphanumeric() || c == b'_')
                {
                    self.pos += 1;
                }
                Token::Ident(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
            }
            b'{' | b'}' | b'[' | b']' | b'=' | b',' | b';' | b'(' | b')' | b'-' => {
                self.pos += 1;
                Token::Sym(c as char)
            }
            other => return Err(self.err(format!("unexpected character {:?}", other as char))),
        };
        Ok(Some(token))
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    peeked: Option<Option<Token>>,
}

impl<'a> Parser<'a> {
    fn peek(&mut self) -> Result<Option<&Token>, Error> {
        if self.peeked.is_none() {
            self.peeked = Some(self.lexer.next()?);
        }
        Ok(self.peeked.as_ref().and_then(|t| t.as_ref()))
    }

    fn bump(&mut self) -> Result<Option<Token>, Error> {
        match self.peeked.take() {
            Some(t) => Ok(t),
            None => self.lexer.next(),
        }
    }

    fn expect_sym(&mut self, sym: char) -> Result<(), Error> {
        match self.bump()? {
            Some(Token::Sym(c)) if c == sym => Ok(()),
            other => Err(self.lexer.err(format!("expected '{sym}', found {other:?}"))),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Option<Value>, Error> {
        if depth > MAX_DEPTH {
            return Err(self.lexer.err("tables nested too deeply"));
        }
        let token = self
            .bump()?
            .ok_or_else(|| self.lexer.err("unexpected end of input"))?;
        Ok(match token {
            Token::Str(s) => Some(Value::Text(s)),
            Token::Num(n) => Some(Value::Number(n)),
            Token::Sym('-') => match self.bump()? {
                Some(Token::Num(n)) => Some(Value::Number(-n)),
                other => {
                    return Err(self
                        .lexer
                        .err(format!("expected number after '-', found {other:?}")));
                }
            },
            Token::Sym('{') => Some(self.table(depth + 1)?),
            Token::Ident(id) => match id.as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                "nil" => None,
                // Localisation wrappers: `LOC "..."`, `ZSTR "..."`, `LOC("...")`.
                _ => self.wrapper_call()?.map(Value::Text),
            },
            other => return Err(self.lexer.err(format!("unexpected token {other:?}"))),
        })
    }

    /// After an identifier, accept a string argument with or without parentheses.
    /// `ZSTR "$$$/Path=Display name"` yields the text after `=` when present.
    fn wrapper_call(&mut self) -> Result<Option<String>, Error> {
        let parenthesised = matches!(self.peek()?, Some(Token::Sym('(')));
        if parenthesised {
            self.bump()?;
        }
        let text = match self.bump()? {
            Some(Token::Str(s)) => s,
            other => {
                return Err(self
                    .lexer
                    .err(format!("expected string argument, found {other:?}")));
            }
        };
        if parenthesised {
            self.expect_sym(')')?;
        }
        let display = match text.strip_prefix("$$$/") {
            Some(rest) => rest
                .split_once('=')
                .map_or(text.clone(), |(_, name)| name.to_string()),
            None => text,
        };
        Ok(Some(display))
    }

    fn table(&mut self, depth: usize) -> Result<Value, Error> {
        let mut keyed: BTreeMap<String, Value> = BTreeMap::new();
        let mut array: Vec<Value> = Vec::new();
        loop {
            match self.peek()? {
                Some(Token::Sym('}')) => {
                    self.bump()?;
                    break;
                }
                Some(Token::Sym(',' | ';')) => {
                    self.bump()?;
                    continue;
                }
                None => return Err(self.lexer.err("unterminated table")),
                _ => {}
            }
            // `[key] = value`
            if matches!(self.peek()?, Some(Token::Sym('['))) {
                self.bump()?;
                let key = match self.bump()? {
                    Some(Token::Str(s)) => s,
                    Some(Token::Num(n)) => format_index(n),
                    other => return Err(self.lexer.err(format!("bad table key {other:?}"))),
                };
                self.expect_sym(']')?;
                self.expect_sym('=')?;
                if let Some(v) = self.value(depth)? {
                    keyed.insert(key, v);
                }
                continue;
            }
            // `name = value` vs. positional value starting with an identifier.
            if let Some(Token::Ident(name)) = self.peek()?.cloned() {
                if !matches!(name.as_str(), "true" | "false" | "nil") {
                    self.bump()?;
                    if matches!(self.peek()?, Some(Token::Sym('='))) {
                        self.bump()?;
                        if let Some(v) = self.value(depth)? {
                            keyed.insert(name, v);
                        }
                    } else if let Some(text) = self.wrapper_call()? {
                        array.push(Value::Text(text));
                    }
                    continue;
                }
            }
            if let Some(v) = self.value(depth)? {
                array.push(v);
            }
        }
        Ok(match (keyed.is_empty(), array.is_empty()) {
            (true, false) => Value::Seq(array),
            (_, true) => Value::Struct(keyed),
            (false, false) => {
                for (i, v) in array.into_iter().enumerate() {
                    keyed.insert(format_index((i + 1) as f64), v);
                }
                Value::Struct(keyed)
            }
        })
    }
}

fn format_index(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("[{}]", n as i64)
    } else {
        format!("[{n}]")
    }
}

/// Parse a `.lrtemplate` file into its top-level table (the value assigned to `s`,
/// or a bare/`return`ed table).
pub fn parse(input: &str) -> Result<Value, Error> {
    let input = input.trim_start_matches('\u{feff}');
    let mut parser = Parser {
        lexer: Lexer::new(input),
        peeked: None,
    };
    match parser.peek()?.cloned() {
        Some(Token::Ident(id)) if id == "return" => {
            parser.bump()?;
        }
        Some(Token::Ident(_)) => {
            parser.bump()?;
            parser.expect_sym('=')?;
        }
        _ => {}
    }
    parser.expect_sym('{')?;
    let table = parser.table(1)?;
    if let Some(extra) = parser.bump()? {
        return Err(parser.lexer.err(format!("trailing content {extra:?}")));
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_preset() {
        let src = r#"s = {
	id = "3E6E5A06",
	internalName = "Warm Matte",
	title = ZSTR "$$$/AgDevelop/Presets/WarmMatte=Warm Matte",
	type = "Develop",
	value = {
		settings = {
			Exposure2012 = 0.35, -- comment
			Contrast2012 = -12,
			ConvertToGrayscale = false,
			ToneCurvePV2012 = { 0, 18, 255, 245, },
			Name = "with \"quotes\"",
		},
		uuid = "3E6E5A06",
	},
	version = 0,
}"#;
        let v = parse(src).unwrap();
        let top = v.as_struct().unwrap();
        assert_eq!(top["title"].as_str(), Some("Warm Matte"));
        let settings = top["value"].as_struct().unwrap()["settings"]
            .as_struct()
            .unwrap();
        assert_eq!(settings["Exposure2012"].as_f64(), Some(0.35));
        assert_eq!(settings["Contrast2012"].as_f64(), Some(-12.0));
        assert_eq!(settings["ConvertToGrayscale"].as_bool(), Some(false));
        assert_eq!(settings["ToneCurvePV2012"].as_seq().unwrap().len(), 4);
        assert_eq!(settings["Name"].as_str(), Some("with \"quotes\""));
    }

    #[test]
    fn bracket_keys_long_strings_and_loc() {
        let src = r#"return { ["a b"] = [[long
text]], [2] = 7, title = LOC("$$$/X=Hello"), 1e3 }"#;
        let v = parse(src).unwrap();
        let t = v.as_struct().unwrap();
        assert_eq!(t["a b"].as_str(), Some("long\ntext"));
        assert_eq!(t["[2]"].as_f64(), Some(7.0));
        assert_eq!(t["title"].as_str(), Some("Hello"));
        assert_eq!(t["[1]"].as_f64(), Some(1000.0));
    }

    #[test]
    fn rejects_garbage_and_deep_nesting() {
        assert!(parse("s = { a = }").is_err());
        assert!(parse("s = { a = 1 ").is_err());
        let deep = format!("s = {}{}", "{".repeat(200), "}".repeat(200));
        assert!(parse(&deep).is_err());
    }
}
