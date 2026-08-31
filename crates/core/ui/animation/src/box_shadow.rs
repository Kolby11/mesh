//! `box-shadow` parsing and interpolation for the animation API.
//!
//! The public animation parser intentionally supports one shadow and rejects
//! comma-separated lists with a structured error. Rendering consumes the
//! canonical style value after the shell has validated and lowered it.

use mesh_core_elements::style::Color;
use std::fmt;

use super::interpolate::Interpolate;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: Color,
    pub inset: bool,
}

impl BoxShadow {
    pub const NONE: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::TRANSPARENT,
        inset: false,
    };

    pub fn is_none(self) -> bool {
        self.color.a == 0
            && self.offset_x == 0.0
            && self.offset_y == 0.0
            && self.blur_radius == 0.0
            && self.spread_radius == 0.0
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self::NONE
    }
}

impl Interpolate for BoxShadow {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Self {
            offset_x: self.offset_x.lerp(other.offset_x, progress),
            offset_y: self.offset_y.lerp(other.offset_y, progress),
            blur_radius: self.blur_radius.lerp(other.blur_radius, progress),
            spread_radius: self.spread_radius.lerp(other.spread_radius, progress),
            color: self.color.lerp(other.color, progress),
            // `inset` doesn't interpolate; snap at midpoint.
            inset: if progress < 0.5 {
                self.inset
            } else {
                other.inset
            },
        }
    }
}

/// A structured failure from [`parse_box_shadow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxShadowParseError {
    EmptyValue,
    MultipleShadows,
    UnbalancedFunction,
    InvalidToken(String),
    InvalidLength(String),
    InvalidColor(String),
    DuplicateInset,
    DuplicateColor,
    MissingOffsets,
    MissingColor,
    TooManyLengths,
    NegativeBlur,
}

impl fmt::Display for BoxShadowParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyValue => formatter.write_str("box-shadow value is empty"),
            Self::MultipleShadows => {
                formatter.write_str("multiple box-shadow values are not supported")
            }
            Self::UnbalancedFunction => {
                formatter.write_str("box-shadow has unbalanced parentheses")
            }
            Self::InvalidToken(token) => write!(formatter, "invalid box-shadow token `{token}`"),
            Self::InvalidLength(token) => write!(formatter, "invalid box-shadow length `{token}`"),
            Self::InvalidColor(token) => write!(formatter, "invalid box-shadow color `{token}`"),
            Self::DuplicateInset => {
                formatter.write_str("box-shadow contains `inset` more than once")
            }
            Self::DuplicateColor => {
                formatter.write_str("box-shadow contains a color more than once")
            }
            Self::MissingOffsets => formatter.write_str("box-shadow requires x and y offsets"),
            Self::MissingColor => formatter.write_str("box-shadow requires a color"),
            Self::TooManyLengths => formatter.write_str("box-shadow has more than four lengths"),
            Self::NegativeBlur => formatter.write_str("box-shadow blur radius cannot be negative"),
        }
    }
}

impl std::error::Error for BoxShadowParseError {}

/// Parse one CSS `box-shadow` value.
///
/// The animation API currently stores one shadow, so comma-separated shadow
/// lists are rejected explicitly instead of being truncated. Accepted values
/// use the strict `[inset] <ox> <oy> <blur>? <spread>? <color>` form, with
/// pixel lengths (or unitless zero) and the CSS color forms supported by the
/// element style contract.
pub fn parse_box_shadow(value: &str) -> Result<BoxShadow, BoxShadowParseError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(BoxShadowParseError::EmptyValue);
    }
    if value.eq_ignore_ascii_case("none") {
        return Ok(BoxShadow::NONE);
    }

    let tokens = tokenize(value)?;
    let mut inset = false;
    let mut color = None;
    let mut lengths = Vec::with_capacity(4);

    for token in tokens {
        if token.eq_ignore_ascii_case("inset") {
            if inset {
                return Err(BoxShadowParseError::DuplicateInset);
            }
            inset = true;
        } else if let Some(parsed) = Color::from_css(&token) {
            if color.replace(parsed).is_some() {
                return Err(BoxShadowParseError::DuplicateColor);
            }
        } else if token.starts_with('#') || token.starts_with("rgb") {
            return Err(BoxShadowParseError::InvalidColor(token));
        } else if let Some(parsed) = parse_length(&token) {
            lengths.push(parsed);
        } else if looks_like_length(&token) {
            return Err(BoxShadowParseError::InvalidLength(token));
        } else {
            return Err(BoxShadowParseError::InvalidToken(token));
        }
    }

    if lengths.len() < 2 {
        return Err(BoxShadowParseError::MissingOffsets);
    }
    if lengths.len() > 4 {
        return Err(BoxShadowParseError::TooManyLengths);
    }
    let color = color.ok_or(BoxShadowParseError::MissingColor)?;
    if lengths.get(2).is_some_and(|blur| *blur < 0.0) {
        return Err(BoxShadowParseError::NegativeBlur);
    }

    Ok(BoxShadow {
        offset_x: lengths[0],
        offset_y: lengths[1],
        blur_radius: lengths.get(2).copied().unwrap_or(0.0),
        spread_radius: lengths.get(3).copied().unwrap_or(0.0),
        color,
        inset,
    })
}

fn tokenize(value: &str) -> Result<Vec<String>, BoxShadowParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut parentheses = 0usize;

    for character in value.chars() {
        match character {
            '(' => {
                parentheses += 1;
                current.push(character);
            }
            ')' => {
                if parentheses == 0 {
                    return Err(BoxShadowParseError::UnbalancedFunction);
                }
                parentheses -= 1;
                current.push(character);
            }
            ',' if parentheses == 0 => return Err(BoxShadowParseError::MultipleShadows),
            character if character.is_whitespace() && parentheses == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }

    if parentheses != 0 {
        return Err(BoxShadowParseError::UnbalancedFunction);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return Err(BoxShadowParseError::EmptyValue);
    }
    Ok(tokens)
}

fn parse_length(token: &str) -> Option<f32> {
    let raw = token.strip_suffix("px").unwrap_or(token);
    let value = raw.parse::<f32>().ok()?;
    if !value.is_finite() || (!token.ends_with("px") && value != 0.0) {
        return None;
    }
    Some(value)
}

fn looks_like_length(token: &str) -> bool {
    token.ends_with("px")
        || token.starts_with('+')
        || token.starts_with('-')
        || token.starts_with('.')
        || token.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_shadow_with_function_color_and_inset() {
        let shadow = parse_box_shadow("inset -2px 3px 4px 5px rgba(10, 20, 30, 0.5)").unwrap();

        assert_eq!(shadow.offset_x, -2.0);
        assert_eq!(shadow.offset_y, 3.0);
        assert_eq!(shadow.blur_radius, 4.0);
        assert_eq!(shadow.spread_radius, 5.0);
        assert_eq!(
            shadow.color,
            Color {
                r: 10,
                g: 20,
                b: 30,
                a: 128
            }
        );
        assert!(shadow.inset);
    }

    #[test]
    fn parses_optional_lengths_and_color_in_any_order() {
        let shadow = parse_box_shadow("#123456 0 2px").unwrap();

        assert_eq!(shadow.offset_x, 0.0);
        assert_eq!(shadow.offset_y, 2.0);
        assert_eq!(shadow.blur_radius, 0.0);
        assert_eq!(shadow.spread_radius, 0.0);
        assert_eq!(shadow.color, Color::from_hex("#123456").unwrap());
        assert!(!shadow.inset);
    }

    #[test]
    fn parses_none_without_constructing_a_shadow() {
        assert_eq!(parse_box_shadow("none").unwrap(), BoxShadow::NONE);
    }

    #[test]
    fn rejects_unsupported_or_malformed_values_structurally() {
        assert_eq!(parse_box_shadow(""), Err(BoxShadowParseError::EmptyValue));
        assert_eq!(
            parse_box_shadow("0 2px 4px #000, 1px 1px 2px #fff"),
            Err(BoxShadowParseError::MultipleShadows)
        );
        assert_eq!(
            parse_box_shadow("0 2px -4px #000"),
            Err(BoxShadowParseError::NegativeBlur)
        );
        assert_eq!(
            parse_box_shadow("0 2px 4px"),
            Err(BoxShadowParseError::MissingColor)
        );
        assert_eq!(
            parse_box_shadow("0 2px 4px #000 #fff"),
            Err(BoxShadowParseError::DuplicateColor)
        );
        assert_eq!(
            parse_box_shadow("0 2px 4px #000 1em"),
            Err(BoxShadowParseError::InvalidLength("1em".into()))
        );
    }

    #[test]
    fn rejects_unbalanced_functions_and_duplicate_keywords() {
        assert_eq!(
            parse_box_shadow("0 2px rgb(0, 0, 0 #000"),
            Err(BoxShadowParseError::UnbalancedFunction)
        );
        assert_eq!(
            parse_box_shadow("inset inset 0 2px #000"),
            Err(BoxShadowParseError::DuplicateInset)
        );
    }

    #[test]
    fn section9_box_shadow_parser_matrix_returns_structured_errors() {
        let cases = [
            ("", BoxShadowParseError::EmptyValue),
            (
                "0 0 #000, 1px 1px #fff",
                BoxShadowParseError::MultipleShadows,
            ),
            (
                "0 0 rgb(0, 0, 0 #000",
                BoxShadowParseError::UnbalancedFunction,
            ),
            ("0 0 #ggg", BoxShadowParseError::InvalidColor("#ggg".into())),
            (
                "0 0 1em #000",
                BoxShadowParseError::InvalidLength("1em".into()),
            ),
            ("0 0 nope", BoxShadowParseError::InvalidToken("nope".into())),
            ("inset inset 0 0 #000", BoxShadowParseError::DuplicateInset),
            ("0 0 #000 #fff", BoxShadowParseError::DuplicateColor),
            ("0 #000", BoxShadowParseError::MissingOffsets),
            ("0 0 1px", BoxShadowParseError::MissingColor),
            ("0 0 1px 2px 3px #000", BoxShadowParseError::TooManyLengths),
            ("0 0 -1px #000", BoxShadowParseError::NegativeBlur),
        ];

        for (value, expected) in cases {
            let error = parse_box_shadow(value).expect_err(value);
            assert_eq!(error, expected, "unexpected parse result for {value:?}");
            assert!(error.to_string().contains("box-shadow"));
        }
    }

    #[test]
    fn section9_box_shadow_interpolation_matrix_preserves_supported_components() {
        let from = parse_box_shadow("0 0 0 0 #00000000").unwrap();
        let to = parse_box_shadow("10px 20px 30px 4px rgba(255, 128, 0, 0.5) inset").unwrap();

        let midpoint = from.lerp(to, 0.5);
        assert_eq!(midpoint.offset_x, 5.0);
        assert_eq!(midpoint.offset_y, 10.0);
        assert_eq!(midpoint.blur_radius, 15.0);
        assert_eq!(midpoint.spread_radius, 2.0);
        assert_eq!(
            midpoint.color,
            Color {
                r: 128,
                g: 64,
                b: 0,
                a: 64,
            }
        );
        assert!(midpoint.inset);
        assert!(!from.lerp(to, 0.49).inset);
    }
}
