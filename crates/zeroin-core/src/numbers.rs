//! Locale-safe number entry.
//!
//! Players type decimals with either a point or a comma ("0.35" or "0,35"),
//! and some write thousands with a separator ("1.600" or "1,600" DPI). These
//! helpers accept both without guessing silently: when a value was read in a
//! way the player may not expect, the caller gets a note to show.

/// Why text could not be read as a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberError {
    /// Nothing typed.
    Empty,
    /// Not a number.
    Invalid,
    /// Both "." and "," appear, so the decimal separator is ambiguous.
    MixedSeparators,
}

impl NumberError {
    /// Plain-language message.
    pub fn message(self) -> &'static str {
        match self {
            NumberError::Empty => "Enter a number.",
            NumberError::Invalid => "That is not a number.",
            NumberError::MixedSeparators => "Use one decimal separator, for example 0.35 or 0,35, without thousands separators.",
        }
    }
}

/// A parsed value and whether a separator was interpreted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parsed {
    /// The value.
    pub value: f64,
    /// True when a comma was read as the decimal point, or a separator as thousands.
    pub reinterpreted: bool,
}

fn clean(s: &str) -> Result<String, NumberError> {
    let t: String = s.trim().chars().filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '_' | '\'')).collect();
    if t.is_empty() {
        return Err(NumberError::Empty);
    }
    Ok(t)
}

fn finite(v: Result<f64, std::num::ParseFloatError>) -> Result<f64, NumberError> {
    match v {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(NumberError::Invalid),
    }
}

/// Parse a decimal such as a sensitivity: "0.35", "0,35" and ".35" all give 0.35.
pub fn parse_decimal(s: &str) -> Result<Parsed, NumberError> {
    let t = clean(s)?;
    let (dots, commas) = (t.matches('.').count(), t.matches(',').count());
    if dots > 0 && commas > 0 {
        return Err(NumberError::MixedSeparators);
    }
    if dots > 1 || commas > 1 {
        return Err(NumberError::Invalid);
    }
    if t.chars().any(|c| !(c.is_ascii_digit() || c == '.' || c == ',')) {
        return Err(NumberError::Invalid);
    }
    let value = finite(t.replace(',', ".").parse())?;
    Ok(Parsed { value, reinterpreted: commas == 1 })
}

/// Parse a mouse DPI. Whole numbers only; "1.600", "1,600" and "1 600" are
/// read as 1600 (DPI is never fractional) and flagged so the UI can say so.
pub fn parse_dpi(s: &str) -> Result<Parsed, NumberError> {
    let raw = s.trim();
    let t = clean(raw)?;
    let spaced = raw.chars().any(|c| matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\''));
    if t.chars().all(|c| c.is_ascii_digit()) {
        return Ok(Parsed { value: finite(t.parse())?, reinterpreted: spaced });
    }
    // Thousands groups: 1–2 leading digits, then groups of exactly three.
    for sep in ['.', ','] {
        let parts: Vec<&str> = t.split(sep).collect();
        let other = if sep == '.' { ',' } else { '.' };
        if parts.len() >= 2
            && !t.contains(other)
            && (1..=2).contains(&parts[0].len())
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
            && parts[1..].iter().all(|p| p.len() == 3)
        {
            return Ok(Parsed { value: finite(parts.concat().parse())?, reinterpreted: true });
        }
    }
    // Otherwise allow a plain decimal ("800.0").
    let p = parse_decimal(&t)?;
    Ok(Parsed { value: p.value, reinterpreted: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_with_point_or_comma() {
        assert_eq!(parse_decimal("0.35").unwrap(), Parsed { value: 0.35, reinterpreted: false });
        assert_eq!(parse_decimal(" 0,35 ").unwrap(), Parsed { value: 0.35, reinterpreted: true });
        assert_eq!(parse_decimal(".5").unwrap().value, 0.5);
        assert_eq!(parse_decimal("1.234,5"), Err(NumberError::MixedSeparators));
        assert_eq!(parse_decimal("1.2.3"), Err(NumberError::Invalid));
        assert_eq!(parse_decimal("abc"), Err(NumberError::Invalid));
        assert_eq!(parse_decimal("-1"), Err(NumberError::Invalid));
        assert_eq!(parse_decimal("inf"), Err(NumberError::Invalid));
        assert_eq!(parse_decimal("  "), Err(NumberError::Empty));
    }

    #[test]
    fn dpi_thousands_separators() {
        assert_eq!(parse_dpi("1600").unwrap(), Parsed { value: 1600.0, reinterpreted: false });
        for s in ["1.600", "1,600", "1 600", "1\u{a0}600"] {
            assert_eq!(parse_dpi(s).unwrap(), Parsed { value: 1600.0, reinterpreted: true }, "{s}");
        }
        assert_eq!(parse_dpi("12.000").unwrap().value, 12000.0);
        assert_eq!(parse_dpi("800.0").unwrap().value, 800.0);
        assert_eq!(parse_dpi("1,6").unwrap().value, 1.6);
        assert!(parse_dpi("1.600,5").is_err());
    }
}
