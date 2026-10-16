use full_moon::tokenizer::{StringLiteralQuoteType, TokenReference, TokenType};

pub(super) fn string(token: &TokenReference) -> Result<String, String> {
    let TokenType::StringLiteral {
        literal,
        quote_type,
        ..
    } = token.token_type()
    else {
        return Err("expected a string literal".into());
    };
    if literal.contains('\0') {
        return Err("raw NUL bytes are not valid Luau string source".into());
    }
    if *quote_type == StringLiteralQuoteType::Brackets {
        // Luau normalizes CRLF only; a standalone carriage return is data.
        let normalized = literal.replace("\r\n", "\n");
        return Ok(normalized
            .strip_prefix('\n')
            .unwrap_or(&normalized)
            .to_string());
    }
    let bytes = literal.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte != b'\\' {
            if matches!(byte, b'\r' | b'\n') {
                return Err("unescaped physical newline in a quoted Luau string".into());
            }
            output.push(byte);
            continue;
        }
        let escaped = *bytes.get(index).ok_or("unterminated string escape")?;
        index += 1;
        match escaped {
            b'a' => output.push(7),
            b'b' => output.push(8),
            b'f' => output.push(12),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'v' => output.push(11),
            b'\\' | b'"' | b'\'' => output.push(escaped),
            b'\r' | b'\n' => {
                if escaped == b'\r' && bytes.get(index) == Some(&b'\n') {
                    index += 1;
                }
                output.push(b'\n');
            }
            b'z' => {
                while bytes
                    .get(index)
                    .is_some_and(|byte| matches!(*byte, b' ' | b'\t'..=b'\r'))
                {
                    index += 1;
                }
            }
            b'x' => {
                let hex = bytes
                    .get(index..index + 2)
                    .ok_or("hex escape requires two digits")?;
                let hex = std::str::from_utf8(hex).map_err(|_| "invalid hex escape")?;
                output.push(u8::from_str_radix(hex, 16).map_err(|_| "invalid hex escape")?);
                index += 2;
            }
            b'u' => {
                if bytes.get(index) != Some(&b'{') {
                    return Err("unicode escape requires `{...}`".into());
                }
                index += 1;
                let end = bytes[index..]
                    .iter()
                    .position(|byte| *byte == b'}')
                    .map(|end| index + end)
                    .ok_or("unterminated unicode escape")?;
                let hex = std::str::from_utf8(&bytes[index..end])
                    .map_err(|_| "invalid unicode escape")?;
                let scalar = u32::from_str_radix(hex, 16).map_err(|_| "invalid unicode escape")?;
                let character =
                    char::from_u32(scalar).ok_or("unicode escape is not a Unicode scalar")?;
                let mut buffer = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                index = end + 1;
            }
            b'0'..=b'9' => {
                let mut value = (escaped - b'0') as u16;
                for _ in 0..2 {
                    let Some(byte @ b'0'..=b'9') = bytes.get(index) else {
                        break;
                    };
                    value = value * 10 + (byte - b'0') as u16;
                    index += 1;
                }
                output.push(u8::try_from(value).map_err(|_| "decimal string escape exceeds 255")?);
            }
            _ => output.push(escaped),
        }
    }
    String::from_utf8(output).map_err(|_| "prop strings must contain valid UTF-8".into())
}

pub(super) fn number(token: &TokenReference) -> Result<f64, String> {
    let text = token.token().to_string().replace('_', "");
    let value = if let Some(binary) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        u64::from_str_radix(binary, 2)
            .ok()
            .map(|number| number as f64)
    } else if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
            .ok()
            .map(|number| number as f64)
    } else {
        text.parse::<f64>().ok()
    };
    value.filter(|value| value.is_finite()).ok_or_else(|| {
        "prop numbers must be finite numeric literals within Luau's literal range".into()
    })
}
