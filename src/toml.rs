//! A small TOML reader and in-place editor.
//!
//! The config file only uses part of TOML, and this module implements that
//! part: comments, `[tables]`, bare, quoted and dotted keys, basic and
//! literal strings, integers, floats, booleans and arrays. Multi-line
//! strings, inline tables, arrays of tables and dates are rejected with an
//! error that says so; nothing is ever silently misread.
//!
//! Besides parsing, [`set`] changes one value in the text of a file and
//! leaves every other byte, comments included, where it was.

use std::fmt;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<Value>),
}

/// One `key = value` line, with the key spelled out from the root.
#[derive(Debug)]
pub struct Entry {
    pub key: Vec<String>,
    pub value: Value,
    pub line: usize,
    /// Where the value sits in the source text.
    span: Range<usize>,
}

/// A `[table]` header. The root of the file is the table with an empty path.
#[derive(Debug)]
pub struct Table {
    pub path: Vec<String>,
    pub line: usize,
    /// Offset just past the table's last line, where a new key would go.
    end: usize,
}

#[derive(Debug, Default)]
pub struct Document {
    pub entries: Vec<Entry>,
    pub tables: Vec<Table>,
}

#[derive(Debug, PartialEq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

pub fn parse(source: &str) -> Result<Document, Error> {
    let mut parser = Parser {
        source,
        position: 0,
        line: 1,
    };
    let mut document = Document {
        entries: Vec::new(),
        tables: vec![Table {
            path: Vec::new(),
            line: 1,
            end: 0,
        }],
    };
    let mut current = 0;

    loop {
        parser.skip_blank_lines();
        let line = parser.line;
        match parser.peek() {
            None => break,
            Some('[') => {
                parser.advance();
                if parser.peek() == Some('[') {
                    return Err(parser.error("arrays of tables ([[name]]) are not supported"));
                }
                let path = parser.key()?;
                parser.expect(']')?;
                parser.end_of_line()?;
                if document.tables.iter().any(|table| table.path == path) {
                    return Err(Error {
                        line,
                        message: format!("table [{}] is defined twice", path.join(".")),
                    });
                }
                current = document.tables.len();
                document.tables.push(Table {
                    path,
                    line,
                    end: parser.position,
                });
            }
            Some(_) => {
                let mut key = document.tables[current].path.clone();
                key.extend(parser.key()?);
                parser.expect('=')?;
                parser.skip_spaces();
                let start = parser.position;
                let value = parser.value()?;
                let span = start..parser.position;
                parser.end_of_line()?;
                if document.entries.iter().any(|entry| entry.key == key) {
                    return Err(Error {
                        line,
                        message: format!("key '{}' is set twice", key.join(".")),
                    });
                }
                document.entries.push(Entry {
                    key,
                    value,
                    line,
                    span,
                });
                document.tables[current].end = parser.position;
            }
        }
    }
    Ok(document)
}

/// Parses text that consists of exactly one value, as given to `--set`.
pub fn parse_value(text: &str) -> Option<Value> {
    let mut parser = Parser {
        source: text,
        position: 0,
        line: 1,
    };
    parser.skip_spaces();
    let value = parser.value().ok()?;
    parser.skip_spaces();
    parser.peek().is_none().then_some(value)
}

/// Returns `source` with `key` set to `value`.
///
/// An existing value is replaced where it stands. A new key is added at the
/// end of its table, and a missing table is appended to the file.
pub fn set(source: &str, key: &[&str], value: &Value) -> Result<String, Error> {
    let document = parse(source)?;
    let (leaf, parents) = key.split_last().expect("keys are never empty");
    let matches = |path: &[String], wanted: &[&str]| {
        path.iter().map(String::as_str).eq(wanted.iter().copied())
    };

    let mut text = source.to_string();
    if let Some(entry) = document
        .entries
        .iter()
        .find(|entry| matches(&entry.key, key))
    {
        text.replace_range(entry.span.clone(), &value.to_toml());
        return Ok(text);
    }

    let line = format!("{} = {}\n", format_key(leaf), value.to_toml());
    match document
        .tables
        .iter()
        .find(|table| matches(&table.path, parents))
    {
        Some(table) => {
            // A last line without a newline has to be finished first.
            let unterminated = table.end > 0 && !text[..table.end].ends_with('\n');
            let line = if unterminated {
                format!("\n{line}")
            } else {
                line
            };
            text.insert_str(table.end, &line);
        }
        None => {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            let header: Vec<String> = parents.iter().map(|part| format_key(part)).collect();
            text.push_str(&format!("\n[{}]\n{line}", header.join(".")));
        }
    }
    Ok(text)
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "a string",
            Value::Integer(_) => "an integer",
            Value::Float(_) => "a number",
            Value::Boolean(_) => "true or false",
            Value::Array(_) => "an array",
        }
    }

    pub fn to_toml(&self) -> String {
        match self {
            Value::String(text) => quote(text),
            Value::Integer(number) => number.to_string(),
            Value::Float(number) => format!("{number:?}"),
            Value::Boolean(flag) => flag.to_string(),
            Value::Array(items) => {
                let items: Vec<String> = items.iter().map(Value::to_toml).collect();
                // Long lists read better with one item per line.
                if items.len() > 3 {
                    format!("[\n    {},\n]", items.join(",\n    "))
                } else {
                    format!("[{}]", items.join(", "))
                }
            }
        }
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Value {
        Value::String(text.to_string())
    }
}

/// `text` as a TOML basic string.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn is_bare_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn format_key(key: &str) -> String {
    if !key.is_empty() && key.chars().all(is_bare_key_char) {
        key.to_string()
    } else {
        quote(key)
    }
}

struct Parser<'a> {
    source: &'a str,
    position: usize,
    line: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.source[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += c.len_utf8();
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error {
            line: self.line,
            message: message.into(),
        }
    }

    fn expect(&mut self, wanted: char) -> Result<(), Error> {
        self.skip_spaces();
        match self.peek() {
            Some(c) if c == wanted => {
                self.advance();
                Ok(())
            }
            Some(c) => Err(self.error(format!("expected '{wanted}', found '{c}'"))),
            None => Err(self.error(format!("expected '{wanted}', found the end of the file"))),
        }
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.advance();
        }
    }

    fn skip_comment(&mut self) {
        if self.peek() == Some('#') {
            while !matches!(self.peek(), Some('\n') | None) {
                self.advance();
            }
        }
    }

    /// Skips whitespace, comments and line breaks.
    fn skip_blank_lines(&mut self) {
        loop {
            self.skip_spaces();
            self.skip_comment();
            if !matches!(self.peek(), Some('\n' | '\r')) {
                return;
            }
            self.advance();
        }
    }

    /// Consumes the remainder of a line, which may only hold a comment.
    fn end_of_line(&mut self) -> Result<(), Error> {
        self.skip_spaces();
        self.skip_comment();
        if self.peek() == Some('\r') {
            self.advance();
        }
        match self.peek() {
            None => Ok(()),
            Some('\n') => {
                self.advance();
                Ok(())
            }
            Some(c) => Err(self.error(format!("unexpected '{c}' after the value"))),
        }
    }

    /// A key, possibly dotted: `width`, `module.cpu`, `"odd name".label`.
    fn key(&mut self) -> Result<Vec<String>, Error> {
        let mut parts = Vec::new();
        loop {
            self.skip_spaces();
            let part = match self.peek() {
                Some('"') => self.basic_string()?,
                Some('\'') => self.literal_string()?,
                _ => {
                    let length = self
                        .rest()
                        .find(|c| !is_bare_key_char(c))
                        .unwrap_or(self.rest().len());
                    if length == 0 {
                        return Err(self.error("expected a key"));
                    }
                    let part = self.rest()[..length].to_string();
                    self.position += length;
                    part
                }
            };
            parts.push(part);
            self.skip_spaces();
            if self.peek() != Some('.') {
                return Ok(parts);
            }
            self.advance();
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        match self.peek() {
            None | Some('\n' | '\r' | '#') => Err(self.error("expected a value")),
            Some('"') => self.basic_string().map(Value::String),
            Some('\'') => self.literal_string().map(Value::String),
            Some('[') => self.array(),
            Some('{') => Err(self.error("inline tables ({ ... }) are not supported")),
            Some(_) => self.scalar(),
        }
    }

    fn basic_string(&mut self) -> Result<String, Error> {
        if self.rest().starts_with("\"\"\"") {
            return Err(self.error("multi-line strings are not supported"));
        }
        self.advance();
        let mut text = String::new();
        loop {
            // Looked at before it is consumed, so that the error names the
            // line the string started on.
            if matches!(self.peek(), None | Some('\n')) {
                return Err(self.error("string is missing its closing quote"));
            }
            match self.advance() {
                Some('"') => return Ok(text),
                Some('\\') => text.push(self.escape()?),
                Some(c) => text.push(c),
                None => unreachable!("peeked above"),
            }
        }
    }

    fn escape(&mut self) -> Result<char, Error> {
        let unicode = |parser: &mut Self, digits: usize| {
            let code = parser
                .rest()
                .get(..digits)
                .and_then(|hex| u32::from_str_radix(hex, 16).ok());
            match code.and_then(char::from_u32) {
                Some(c) => {
                    parser.position += digits;
                    Ok(c)
                }
                None => Err(parser.error("invalid unicode escape")),
            }
        };
        match self.advance() {
            Some('"') => Ok('"'),
            Some('\\') => Ok('\\'),
            Some('n') => Ok('\n'),
            Some('t') => Ok('\t'),
            Some('r') => Ok('\r'),
            Some('b') => Ok('\u{8}'),
            Some('f') => Ok('\u{c}'),
            Some('e') => Ok('\u{1b}'),
            Some('u') => unicode(self, 4),
            Some('U') => unicode(self, 8),
            Some(c) => Err(self.error(format!(
                "unknown escape '\\{c}' (write \\\\ for a backslash)"
            ))),
            None => Err(self.error("string is missing its closing quote")),
        }
    }

    fn literal_string(&mut self) -> Result<String, Error> {
        if self.rest().starts_with("'''") {
            return Err(self.error("multi-line strings are not supported"));
        }
        self.advance();
        let rest = self.rest();
        match rest.find(['\'', '\n']) {
            Some(end) if rest[end..].starts_with('\'') => {
                let text = rest[..end].to_string();
                self.position += end + 1;
                Ok(text)
            }
            _ => Err(self.error("string is missing its closing quote")),
        }
    }

    fn array(&mut self) -> Result<Value, Error> {
        self.advance();
        let mut items = Vec::new();
        loop {
            self.skip_blank_lines();
            if self.peek() == Some(']') {
                break;
            }
            items.push(self.value()?);
            self.skip_blank_lines();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(']') => break,
                _ => return Err(self.error("expected ',' or ']' in the array")),
            }
        }
        self.advance();
        Ok(Value::Array(items))
    }

    /// A boolean or a number.
    fn scalar(&mut self) -> Result<Value, Error> {
        let rest = self.rest();
        let length = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || "+-._:".contains(c)))
            .unwrap_or(rest.len());
        let word = &rest[..length];
        let digits = word.replace('_', "");
        let value = match word {
            "true" => Value::Boolean(true),
            "false" => Value::Boolean(false),
            _ => {
                let radix = [("0x", 16), ("0o", 8), ("0b", 2)]
                    .into_iter()
                    .find_map(|(prefix, radix)| Some((digits.strip_prefix(prefix)?, radix)));
                let integer = match radix {
                    Some((digits, radix)) => i64::from_str_radix(digits, radix).ok(),
                    None => digits.parse().ok(),
                };
                // Rust's float parser also accepts words such as "infinity",
                // so require the token to look like a number first.
                let numeric = word.starts_with(|c: char| c.is_ascii_digit() || "+-.".contains(c));
                match (integer, digits.parse::<f64>()) {
                    (Some(integer), _) => Value::Integer(integer),
                    (None, Ok(float)) if numeric && !word.contains(':') => Value::Float(float),
                    _ if word.is_empty() => {
                        let found = rest.chars().next().unwrap_or(' ');
                        return Err(self.error(format!("unexpected '{found}'")));
                    }
                    _ => {
                        return Err(self.error(format!(
                            "'{word}' is not a value (strings need quotes; dates are not supported)"
                        )));
                    }
                }
            }
        };
        self.position += length;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(source: &str) -> Vec<(String, Value)> {
        parse(source)
            .unwrap()
            .entries
            .into_iter()
            .map(|entry| (entry.key.join("."), entry.value))
            .collect()
    }

    fn text(value: &str) -> Value {
        Value::String(value.to_string())
    }

    #[test]
    fn parses_scalars() {
        let source = "a = 1\nb = -2_000\nc = 1.5\nd = true\ne = \"x\"\nf = 'y'\ng = 0xff\n";
        assert_eq!(
            entries(source),
            [
                ("a".into(), Value::Integer(1)),
                ("b".into(), Value::Integer(-2000)),
                ("c".into(), Value::Float(1.5)),
                ("d".into(), Value::Boolean(true)),
                ("e".into(), text("x")),
                ("f".into(), text("y")),
                ("g".into(), Value::Integer(255)),
            ]
        );
    }

    #[test]
    fn parses_tables_and_dotted_keys() {
        let source = "top = 1\n[a]\nx = 2 # note\n[a.b]\ny.z = 3\n\n[\"odd name\"]\nk = 4\n";
        assert_eq!(
            entries(source),
            [
                ("top".into(), Value::Integer(1)),
                ("a.x".into(), Value::Integer(2)),
                ("a.b.y.z".into(), Value::Integer(3)),
                ("odd name.k".into(), Value::Integer(4)),
            ]
        );
        let tables = parse("[a]\n[a.b]\n").unwrap().tables;
        assert_eq!(tables.len(), 3);
        assert_eq!(tables[2].path, ["a", "b"]);
    }

    #[test]
    fn parses_strings() {
        let source = r#"
a = "tab\there \"quoted\" \\ \u00e9"
b = 'C:\raw\path'
c = "{#bold}{name}{? ({cores})}"
d = ""
"#;
        assert_eq!(
            entries(source),
            [
                ("a".into(), text("tab\there \"quoted\" \\ é")),
                ("b".into(), text("C:\\raw\\path")),
                ("c".into(), text("{#bold}{name}{? ({cores})}")),
                ("d".into(), text("")),
            ]
        );
    }

    #[test]
    fn parses_arrays() {
        let source = "a = [\n    \"x\", # first\n    \"y\",\n]\nb = []\nc = [1, [2, 3]]\n";
        assert_eq!(
            entries(source),
            [
                ("a".into(), Value::Array(vec![text("x"), text("y")])),
                ("b".into(), Value::Array(vec![])),
                (
                    "c".into(),
                    Value::Array(vec![
                        Value::Integer(1),
                        Value::Array(vec![Value::Integer(2), Value::Integer(3)])
                    ])
                ),
            ]
        );
    }

    #[test]
    fn handles_windows_line_endings() {
        assert_eq!(
            entries("[t]\r\na = 1\r\nb = [\r\n  2,\r\n]\r\n"),
            [
                ("t.a".into(), Value::Integer(1)),
                ("t.b".into(), Value::Array(vec![Value::Integer(2)])),
            ]
        );
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        let line = |source: &str| parse(source).unwrap_err().line;
        assert_eq!(line("a = 1\nb = \n"), 2);
        assert_eq!(line("a = 1\n\n# c\nb = \"open\n"), 4);
        assert_eq!(line("a = [1,\n2\n3]\n"), 3);
        assert_eq!(line("a = 1\na = 2\n"), 2);
        assert_eq!(line("[t]\n[t]\n"), 2);
        assert_eq!(line("a = 1 2\n"), 1);
        assert_eq!(line("= 1\n"), 1);
    }

    #[test]
    fn rejects_what_it_does_not_support() {
        for source in [
            "a = \"\"\"multi\"\"\"\n",
            "a = { b = 1 }\n",
            "[[a]]\n",
            "a = 2026-10-13\n",
            "a = bare\n",
            "a = inf\n",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn parses_standalone_values() {
        assert_eq!(parse_value("20"), Some(Value::Integer(20)));
        assert_eq!(parse_value(" true "), Some(Value::Boolean(true)));
        assert_eq!(parse_value("\" -> \""), Some(text(" -> ")));
        assert_eq!(
            parse_value("[\"os\", \"cpu\"]"),
            Some(Value::Array(vec![text("os"), text("cpu")]))
        );
        assert_eq!(parse_value("blue"), None);
        assert_eq!(parse_value("{name} {version}"), None);
        assert_eq!(parse_value("1 2"), None);
        assert_eq!(parse_value(""), None);
    }

    #[test]
    fn values_round_trip_through_text() {
        let values = [
            text("plain"),
            text("quote \" slash \\ newline \n tab \t"),
            Value::Integer(-7),
            Value::Float(2.5),
            Value::Boolean(false),
            Value::Array(vec![text("a"), text("b")]),
            Value::Array((0..6).map(Value::Integer).collect()),
        ];
        for value in values {
            assert_eq!(parse_value(&value.to_toml()), Some(value));
        }
    }

    #[test]
    fn set_replaces_in_place() {
        let source = "# top\n[theme]\n# above\naccent = \"auto\" # beside\nkeys = \"bold\"\n";
        assert_eq!(
            set(source, &["theme", "accent"], &text("blue")).unwrap(),
            source.replace("\"auto\"", "\"blue\"")
        );
    }

    #[test]
    fn set_adds_missing_keys_and_tables() {
        let source = "a = 1\n\n[bar]\nwidth = 10\n\n[units]\nbinary = true\n";
        assert_eq!(
            set(source, &["bar", "filled"], &text("#")).unwrap(),
            "a = 1\n\n[bar]\nwidth = 10\nfilled = \"#\"\n\n[units]\nbinary = true\n"
        );
        assert_eq!(
            set(source, &["b"], &Value::Integer(2)).unwrap(),
            "a = 1\nb = 2\n\n[bar]\nwidth = 10\n\n[units]\nbinary = true\n"
        );
        assert_eq!(
            set("a = 1", &["module", "my mod", "label"], &text("X")).unwrap(),
            "a = 1\n\n[module.\"my mod\"]\nlabel = \"X\"\n"
        );
        assert_eq!(
            set("[bar]", &["bar", "width"], &Value::Integer(4)).unwrap(),
            "[bar]\nwidth = 4\n"
        );
        assert_eq!(set("", &["a"], &Value::Integer(1)).unwrap(), "a = 1\n");
    }

    #[test]
    fn set_replaces_multi_line_arrays() {
        let source = "modules = [\n    \"os\", # keep?\n    \"cpu\",\n] # after\nx = 1\n";
        let value = Value::Array(vec![text("os")]);
        assert_eq!(
            set(source, &["modules"], &value).unwrap(),
            "modules = [\"os\"] # after\nx = 1\n"
        );
    }
}
