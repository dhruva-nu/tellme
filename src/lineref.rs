//! Parse line selectors used across commands (#30).
//!
//! Accepts the forms shown in EXAMPLES.md and a few conveniences:
//! `#line4`, `#4`, `line4`, `4`, and ranges `#line4-7` / `4-7`.

use crate::error::{Error, Result};

/// Parse a line selector into an inclusive one-based `(start, end)`.
pub fn parse_selector(s: &str) -> Result<(i64, i64)> {
    let bad = || {
        Error::Other(format!(
            "invalid line selector `{s}` (expected e.g. #line4 or 4-7)"
        ))
    };
    let cleaned = s
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("line")
        .trim();
    let parse = |t: &str| t.trim().parse::<i64>().map_err(|_| bad());
    let (start, end) = match cleaned.split_once('-') {
        Some((a, b)) => (parse(a)?, parse(b)?),
        None => {
            let n = parse(cleaned)?;
            (n, n)
        }
    };
    if start < 1 || end < start {
        return Err(bad());
    }
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_supported_forms() {
        assert_eq!(parse_selector("#line4").unwrap(), (4, 4));
        assert_eq!(parse_selector("#4").unwrap(), (4, 4));
        assert_eq!(parse_selector("line4").unwrap(), (4, 4));
        assert_eq!(parse_selector("4").unwrap(), (4, 4));
        assert_eq!(parse_selector("#line4-7").unwrap(), (4, 7));
        assert_eq!(parse_selector("4-7").unwrap(), (4, 7));
    }

    #[test]
    fn rejects_garbage_and_bad_ranges() {
        assert!(parse_selector("0").is_err());
        assert!(parse_selector("7-4").is_err());
        assert!(parse_selector("abc").is_err());
        assert!(parse_selector("").is_err());
    }
}
