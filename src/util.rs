//! Small shared helpers.

use std::fs;
use std::path::{Path, PathBuf};

use lopdf::Document;

/// Every command returns this: a plain boxed error keeps the code short.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Load a PDF, naming the file in the error so failures are readable.
pub fn load(file: &Path) -> Result<Document> {
    Document::load(file).map_err(|error| format!("could not read {}: {error}", file.display()).into())
}

/// Run an edit that writes to a path, and report the path it ended up at.
///
/// With no `output` the file is edited in place: the edit goes to a temporary
/// file next to it first, so a failure leaves the original untouched.
pub fn write_edit(
    file: &Path,
    output: Option<PathBuf>,
    edit: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    match output {
        Some(target) => {
            edit(&target)?;
            Ok(target)
        }
        None => {
            let temporary = file.with_extension("pdftool-tmp");
            edit(&temporary)?;
            fs::rename(&temporary, file)?;
            Ok(file.to_path_buf())
        }
    }
}

/// Parse a page selection like `"1-3,5,8-"` into sorted, unique 1-based page
/// numbers. `total` is the document's page count and bounds open ranges.
pub fn parse_pages(spec: &str, total: u32) -> Result<Vec<u32>> {
    let mut pages = Vec::new();

    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (first, last) = match part.split_once('-') {
            None => {
                let n = number(part, total)?;
                (n, n)
            }
            Some((from, "")) => (number(from, total)?, total),
            Some(("", to)) => (1, number(to, total)?),
            Some((from, to)) => (number(from, total)?, number(to, total)?),
        };

        if first > last {
            return Err(format!("page range '{part}' goes backwards").into());
        }
        pages.extend(first..=last);
    }

    if pages.is_empty() {
        return Err(format!("no pages selected by '{spec}'").into());
    }

    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

fn number(text: &str, total: u32) -> Result<u32> {
    let n: u32 = text
        .trim()
        .parse()
        .map_err(|_| format!("'{text}' is not a page number"))?;

    match n {
        0 => Err("pages start at 1".into()),
        n if n > total => Err(format!("page {n} is out of range, the document has {total}").into()),
        n => Ok(n),
    }
}

/// Turn a PDF date (`D:20240102150405+01'00'`) into `2024-01-02 15:04:05`.
/// Anything unexpected is returned unchanged.
pub fn pretty_date(raw: &str) -> String {
    let digits = raw.trim_start_matches("D:");
    if digits.len() < 14 || !digits[..14].bytes().all(|b| b.is_ascii_digit()) {
        return raw.to_string();
    }
    format!(
        "{}-{}-{} {}:{}:{}",
        &digits[0..4],
        &digits[4..6],
        &digits[6..8],
        &digits[8..10],
        &digits[10..12],
        &digits[12..14]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_and_ranges() {
        assert_eq!(parse_pages("1-3,5", 10).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_pages("8-", 9).unwrap(), vec![8, 9]);
        assert_eq!(parse_pages("-2", 9).unwrap(), vec![1, 2]);
        assert_eq!(parse_pages("3,3,2", 9).unwrap(), vec![2, 3]);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_pages("0", 5).is_err());
        assert!(parse_pages("6", 5).is_err());
        assert!(parse_pages("3-1", 5).is_err());
        assert!(parse_pages("x", 5).is_err());
    }

    #[test]
    fn formats_dates() {
        assert_eq!(pretty_date("D:20240102150405+01'00'"), "2024-01-02 15:04:05");
        assert_eq!(pretty_date("whenever"), "whenever");
    }
}
