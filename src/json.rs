//! Small, bounded JSON parser used at the HTTP boundary.
//!
//! This exists only to avoid silently accepting malformed or unsupported request
//! shapes while the project remains dependency-free. It is not a general serializer.

use std::collections::BTreeMap;
use std::fmt;

const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub offset: usize,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid JSON at byte {}", self.offset)
    }
}

impl std::error::Error for JsonError {}

impl JsonValue {
    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<JsonValue, JsonError> {
    let mut parser = Parser { input, offset: 0 };
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.offset != input.len() {
        return Err(parser.error());
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn error(&self) -> JsonError {
        JsonError { offset: self.offset }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.offset += 1;
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error());
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'n') => {
                self.expect_literal("null")?;
                Ok(JsonValue::Null)
            }
            Some(b't') => {
                self.expect_literal("true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal("false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b'[') => self.parse_array(depth + 1),
            Some(b'{') => self.parse_object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(JsonValue::Number),
            _ => Err(self.error()),
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), JsonError> {
        if self.input[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(())
        } else {
            Err(self.error())
        }
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        if !self.consume(b'"') {
            return Err(self.error());
        }
        let mut output = String::new();
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.offset += 1;
                    return Ok(output);
                }
                Some(b'\\') => {
                    self.offset += 1;
                    match self.peek() {
                        Some(b'"') => {
                            output.push('"');
                            self.offset += 1;
                        }
                        Some(b'\\') => {
                            output.push('\\');
                            self.offset += 1;
                        }
                        Some(b'/') => {
                            output.push('/');
                            self.offset += 1;
                        }
                        Some(b'b') => {
                            output.push('\u{08}');
                            self.offset += 1;
                        }
                        Some(b'f') => {
                            output.push('\u{0c}');
                            self.offset += 1;
                        }
                        Some(b'n') => {
                            output.push('\n');
                            self.offset += 1;
                        }
                        Some(b'r') => {
                            output.push('\r');
                            self.offset += 1;
                        }
                        Some(b't') => {
                            output.push('\t');
                            self.offset += 1;
                        }
                        Some(b'u') => {
                            self.offset += 1;
                            let first = self.parse_hex4()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                if !self.input.as_bytes()[self.offset..].starts_with(b"\\u") {
                                    return Err(self.error());
                                }
                                self.offset += 2;
                                let second = self.parse_hex4()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return Err(self.error());
                                }
                                0x10000 + (((first - 0xd800) as u32) << 10)
                                    + (second - 0xdc00) as u32
                            } else if (0xdc00..=0xdfff).contains(&first) {
                                return Err(self.error());
                            } else {
                                first as u32
                            };
                            output.push(char::from_u32(scalar).ok_or_else(|| self.error())?);
                        }
                        _ => return Err(self.error()),
                    }
                }
                Some(byte) if byte < 0x20 => return Err(self.error()),
                Some(byte) if byte < 0x80 => {
                    output.push(byte as char);
                    self.offset += 1;
                }
                Some(_) => {
                    let character = self.input[self.offset..]
                        .chars()
                        .next()
                        .ok_or_else(|| self.error())?;
                    output.push(character);
                    self.offset += character.len_utf8();
                }
                None => return Err(self.error()),
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u16, JsonError> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(byte @ b'0'..=b'9') => (byte - b'0') as u16,
                Some(byte @ b'a'..=b'f') => (byte - b'a' + 10) as u16,
                Some(byte @ b'A'..=b'F') => (byte - b'A' + 10) as u16,
                _ => return Err(self.error()),
            };
            value = (value << 4) | digit;
            self.offset += 1;
        }
        Ok(value)
    }

    fn parse_array(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.offset += 1;
        self.skip_whitespace();
        let mut values = Vec::new();
        if self.consume(b']') {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.parse_value(depth)?);
            self.skip_whitespace();
            if self.consume(b']') {
                return Ok(JsonValue::Array(values));
            }
            if !self.consume(b',') {
                return Err(self.error());
            }
            self.skip_whitespace();
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.offset += 1;
        self.skip_whitespace();
        let mut values = BTreeMap::new();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(values));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            if !self.consume(b':') {
                return Err(self.error());
            }
            let value = self.parse_value(depth)?;
            if values.insert(key, value).is_some() {
                return Err(self.error());
            }
            self.skip_whitespace();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(values));
            }
            if !self.consume(b',') {
                return Err(self.error());
            }
            self.skip_whitespace();
        }
    }

    fn parse_number(&mut self) -> Result<String, JsonError> {
        let start = self.offset;
        self.consume(b'-');

        match self.peek() {
            Some(b'0') => {
                self.offset += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.error());
                }
            }
            Some(b'1'..=b'9') => {
                self.offset += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.offset += 1;
                }
            }
            _ => return Err(self.error()),
        }

        if self.consume(b'.') {
            let fraction_start = self.offset;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if self.offset == fraction_start {
                return Err(self.error());
            }
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let exponent_start = self.offset;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if self.offset == exponent_start {
                return Err(self.error());
            }
        }

        Ok(self.input[start..self.offset].to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_objects_arrays_scalars_and_unicode_escapes() {
        let value = parse(r#"{"messages":[{"role":"user","content":"hello \uD83D\uDE80"}],"stream":false,"n":-1.25e2}"#).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object["stream"].as_bool(), Some(false));
        assert_eq!(
            object["messages"].as_array().unwrap()[0].as_object().unwrap()["content"].as_str(),
            Some("hello 🚀")
        );
        assert_eq!(object["n"], JsonValue::Number("-1.25e2".to_owned()));
    }

    #[test]
    fn rejects_duplicate_keys_trailing_data_and_bad_numbers() {
        assert!(parse(r#"{"model":"a","model":"b"}"#).is_err());
        assert!(parse(r#"{"model":"a"} trailing"#).is_err());
        assert!(parse("01").is_err());
        assert!(parse("1.").is_err());
        assert!(parse(r#""\uD800""#).is_err());
    }

    #[test]
    fn enforces_nesting_limit() {
        let nested = format!("{}0{}", "[".repeat(MAX_DEPTH + 2), "]".repeat(MAX_DEPTH + 2));
        assert!(parse(&nested).is_err());
    }
}
