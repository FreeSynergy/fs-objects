//! Text normalisation for case-insensitive search and ordering.

use unicode_normalization::UnicodeNormalization;

/// The **sort/search key** of a text: NFC, then full Unicode lowercase (`str::to_lowercase`, which
/// handles final sigma and special mappings), then NFC again (lowercasing can denormalise).
///
/// Choice: this is deterministic and locale-independent, computed in Rust and stored in the
/// `name_sort` / `value_norm` columns, because the SQL engine's `LOWER`/`LIKE` only fold ASCII.
/// Ordering by this key is by code point, not a locale collation: `"Ärger"` sorts after `"Zebra"`
/// (U+00E4 > `z`), and `ß` does not equal `ss`. Language-specific collation (UCA/CLDR) is a
/// possible later addition and would change only this function plus a reindex migration.
pub fn sort_key(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    nfc.to_lowercase().nfc().collect()
}

/// Escape `\`, `%`, `_` for use in `LIKE ... ESCAPE '\'`.
pub(crate) fn like_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_folds_case_and_normalises() {
        assert_eq!(sort_key("NLnet"), "nlnet");
        assert_eq!(sort_key("ÄRGER"), sort_key("ärger"));
        // decomposed vs precomposed
        assert_eq!(sort_key("e\u{301}"), sort_key("\u{e9}"));
        assert!(sort_key("Nextcloud") < sort_key("NLnet"));
    }

    #[test]
    fn like_escape_escapes() {
        assert_eq!(like_escape(r"50%_a\b"), r"50\%\_a\\b");
    }
}
