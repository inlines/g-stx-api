use regex::Regex;
use std::sync::LazyLock;

static BASE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Z]{4})-?([0-9]{5})(.*)$").unwrap());
static VALID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Z]{4}-[0-9]{5}(?:[A-Z]{1,4}|(?:[-/][A-Z0-9]{1,8}){1,3})?$").unwrap()
});
pub const FORMAT_ERROR: &str =
    "Формат: CUSA-12345 (можно без дефиса). Допустим суффикс, например /ANZ или GH.";

pub fn canonical(value: &str) -> String {
    let compact: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '‐' | '‑' | '‒' | '–' | '—' | '−' => '-',
            _ => c,
        })
        .collect::<String>()
        .to_ascii_uppercase();
    BASE.replace(&compact, "$1-$2$3").into_owned()
}
pub fn parse(value: &str) -> Result<String, ()> {
    let value = canonical(value);
    if VALID.is_match(&value) {
        Ok(value)
    } else {
        Err(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[actix_web::test]
    async fn exact_format_and_suffixes() {
        for raw in ["cusa12345", " CUSA 12345 ", "CUSA–12345"] {
            assert_eq!(parse(raw), Ok("CUSA-12345".into()));
        }
        for raw in ["SCES-54330/ANZ", "SLUS-20144GH", "SLPM-65002-0"] {
            assert_eq!(parse(raw).unwrap(), raw);
        }
        for raw in [
            "",
            "CUSA-1234",
            "CUSA-123456",
            "%",
            "CUSA-12345, CUSA-12346",
            "абвг-12345",
        ] {
            assert!(parse(raw).is_err());
        }
    }
}
