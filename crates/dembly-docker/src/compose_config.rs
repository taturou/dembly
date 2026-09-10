use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeServiceConfig {
    /// `None` means that Compose did not override the image value. An empty vector is an explicit override.
    pub entrypoint: Option<Vec<String>>,
    /// `None` means that Compose did not override the image value. An empty vector is an explicit override.
    pub command: Option<Vec<String>>,
    /// `None` means that Compose did not override the image user.
    pub user: Option<String>,
}

pub fn parse_compose_service_config(
    document: &str,
    service: &str,
) -> Result<ComposeServiceConfig, String> {
    let value = JsonParser::new(document).parse()?;
    let services = value
        .object_field("services")
        .ok_or("Compose config JSON has no services object")?;
    let selected = services
        .object_field(service)
        .ok_or_else(|| format!("Compose config has no service {service}"))?;
    Ok(ComposeServiceConfig {
        entrypoint: selected.optional_argv("entrypoint")?,
        command: selected.optional_argv("command")?,
        user: selected.optional_string("user")?,
    })
}

#[derive(Debug)]
enum JsonValue {
    Null,
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
    Scalar,
}

impl JsonValue {
    fn object_field(&self, field: &str) -> Option<&Self> {
        match self {
            Self::Object(fields) => fields.get(field),
            _ => None,
        }
    }

    fn optional_string(&self, field: &str) -> Result<Option<String>, String> {
        match self.object_field(field) {
            None | Some(Self::Null) => Ok(None),
            Some(Self::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(format!("Compose service {field} must be a string or null")),
        }
    }

    fn optional_argv(&self, field: &str) -> Result<Option<Vec<String>>, String> {
        match self.object_field(field) {
            None | Some(Self::Null) => Ok(None),
            Some(Self::String(value)) => Ok(Some(vec![value.clone()])),
            Some(Self::Array(values)) => values
                .iter()
                .map(|value| match value {
                    Self::String(value) => Ok(value.clone()),
                    _ => Err(format!(
                        "Compose service {field} array must contain strings"
                    )),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(format!(
                "Compose service {field} must be an array, string, or null"
            )),
        }
    }
}

struct JsonParser<'a> {
    source: &'a [u8],
    position: usize,
}

impl<'a> JsonParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source: source.as_bytes(),
            position: 0,
        }
    }

    fn parse(mut self) -> Result<JsonValue, String> {
        let value = self.value()?;
        self.whitespace();
        if self.position == self.source.len() {
            Ok(value)
        } else {
            Err("unexpected trailing data in Compose config JSON".into())
        }
    }

    fn value(&mut self) -> Result<JsonValue, String> {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'\"') => self.string().map(JsonValue::String),
            Some(b'n') => self.literal(b"null", JsonValue::Null),
            Some(b't') => self.literal(b"true", JsonValue::Scalar),
            Some(b'f') => self.literal(b"false", JsonValue::Scalar),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err("invalid value in Compose config JSON".into()),
        }
    }

    fn object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.whitespace();
        let mut fields = BTreeMap::new();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            fields.insert(key, value);
            self.whitespace();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(fields));
            }
            self.expect(b',')?;
        }
    }

    fn array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.whitespace();
        let mut values = Vec::new();
        if self.consume(b']') {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.consume(b']') {
                return Ok(JsonValue::Array(values));
            }
            self.expect(b',')?;
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'\"')?;
        let mut value = String::new();
        loop {
            let byte = self
                .next()
                .ok_or("unterminated string in Compose config JSON")?;
            match byte {
                b'\"' => return Ok(value),
                b'\\' => match self.next().ok_or("unterminated JSON escape")? {
                    b'\"' => value.push('\"'),
                    b'\\' => value.push('\\'),
                    b'/' => value.push('/'),
                    b'b' => value.push('\u{0008}'),
                    b'f' => value.push('\u{000c}'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    b'u' => value.push(self.unicode_escape()?),
                    _ => return Err("invalid JSON string escape".into()),
                },
                0..=31 => return Err("control character in JSON string".into()),
                byte if byte.is_ascii() => value.push(char::from(byte)),
                _ => {
                    let start = self.position - 1;
                    let remaining = std::str::from_utf8(&self.source[start..])
                        .map_err(|_| "invalid UTF-8 in Compose config JSON")?;
                    let character = remaining
                        .chars()
                        .next()
                        .ok_or("invalid UTF-8 in Compose config JSON")?;
                    value.push(character);
                    self.position = start + character.len_utf8();
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let scalar = self.unicode_scalar()?;
        if !(0xd800..=0xdbff).contains(&scalar) {
            return char::from_u32(scalar).ok_or_else(|| "invalid JSON unicode scalar".into());
        }
        self.expect(b'\\')?;
        self.expect(b'u')?;
        let low = self.unicode_scalar()?;
        if !(0xdc00..=0xdfff).contains(&low) {
            return Err("invalid JSON unicode surrogate pair".into());
        }
        let scalar = 0x1_0000 + ((scalar - 0xd800) << 10) + (low - 0xdc00);
        char::from_u32(scalar).ok_or_else(|| "invalid JSON unicode surrogate pair".into())
    }

    fn unicode_scalar(&mut self) -> Result<u32, String> {
        let digits = (0..4)
            .map(|_| self.next().ok_or("truncated JSON unicode escape"))
            .collect::<Result<Vec<_>, _>>()?;
        let digits = std::str::from_utf8(&digits).map_err(|_| "invalid JSON unicode escape")?;
        u32::from_str_radix(digits, 16).map_err(|_| "invalid JSON unicode escape".into())
    }

    fn number(&mut self) -> Result<JsonValue, String> {
        let start = self.position;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E')
        }) {
            self.position += 1;
        }
        std::str::from_utf8(&self.source[start..self.position])
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or("invalid number in Compose config JSON")?;
        Ok(JsonValue::Scalar)
    }

    fn literal(&mut self, expected: &[u8], value: JsonValue) -> Result<JsonValue, String> {
        if self
            .source
            .get(self.position..self.position + expected.len())
            == Some(expected)
        {
            self.position += expected.len();
            Ok(value)
        } else {
            Err("invalid literal in Compose config JSON".into())
        }
    }

    fn whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!(
                "expected '{}' in Compose config JSON",
                char::from(expected)
            ))
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.position += 1;
        Some(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_compose_service_config;

    #[test]
    fn parses_service_process_overrides_without_confusing_null_and_empty() {
        let value = parse_compose_service_config(
            r#"{"services":{"dev":{"entrypoint":["/init"],"command":[],"user":"1000:1001"},"other":{"entrypoint":null,"command":null}}}"#,
            "dev",
        )
        .unwrap();

        assert_eq!(value.entrypoint, Some(vec!["/init".into()]));
        assert_eq!(value.command, Some(Vec::new()));
        assert_eq!(value.user, Some("1000:1001".into()));
    }

    #[test]
    fn parses_json_unicode_surrogate_pairs_in_unrelated_config() {
        let value = parse_compose_service_config(
            r#"{"label":"\\ud83d\\ude80","services":{"dev":{"entrypoint":null,"command":["run"],"user":null}}}"#,
            "dev",
        )
        .unwrap();

        assert_eq!(value.command, Some(vec!["run".into()]));
    }
}
