//! Finding and replacing the text inside page content streams.
//!
//! A PDF holds no flowing text: every run of glyphs sits at a fixed position in
//! a font that is usually embedded as a subset of the glyphs the document
//! actually uses. So a replacement
//!
//! * keeps the font, size and position of the text it replaces,
//! * can only use characters that font already carries, and
//! * does not reflow: a longer replacement takes more room than the original.
//!
//! The first two are checked here, and anything that cannot be written cleanly
//! is refused instead of being silently mangled.

use std::collections::BTreeMap;
use std::path::Path;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};

use crate::util::{Result, load};

/// What a replacement changed.
#[derive(Debug, Default)]
pub struct Report {
    pub replaced: usize,
    pub pages: Vec<u32>,
}

/// The text of each page, in page order, for showing to the reader.
pub fn pages(file: &Path) -> Result<Vec<(u32, String)>> {
    let doc = load(file)?;
    let mut pages = Vec::new();

    for number in doc.get_pages().into_keys() {
        let text = doc.extract_text(&[number]).unwrap_or_default();
        pages.push((number, text));
    }
    Ok(pages)
}

/// Replace every occurrence of `search`; an empty `replacement` deletes it.
/// Nothing is written unless the whole document can be rewritten cleanly.
pub fn replace(file: &Path, search: &str, replacement: &str, output: &Path) -> Result<Report> {
    if search.is_empty() {
        return Err("say which text to replace".into());
    }

    let mut doc = load(file)?;
    let mut report = Report::default();

    for (number, page_id) in doc.get_pages() {
        let replaced = replace_in_page(&mut doc, page_id, search, replacement)
            .map_err(|error| format!("page {number}: {error}"))?;
        if replaced > 0 {
            report.replaced += replaced;
            report.pages.push(number);
        }
    }

    if report.replaced == 0 {
        return Err(format!(
            "\"{search}\" not found. Text split across several runs (kerning, ligatures) \
             cannot be matched; try a shorter piece."
        )
        .into());
    }

    doc.save(output)?;
    Ok(report)
}

/// Rewrite one page, returning how many occurrences changed.
fn replace_in_page(doc: &mut Document, page_id: ObjectId, search: &str, replacement: &str) -> Result<usize> {
    // The encodings borrow the document, so the new content is built in here
    // and the document is only modified once that borrow is over.
    let (content, replaced) = {
        let fonts = doc.get_page_fonts(page_id)?;
        // The font dictionary is kept next to its encoding: the encoding says
        // how to write a character, the dictionary says whether the font has it.
        let fonts: BTreeMap<Vec<u8>, (&Dictionary, Encoding)> = fonts
            .into_iter()
            .filter_map(|(name, font)| {
                font.get_font_encoding(doc)
                    .ok()
                    .map(|encoding| (name, (font, encoding)))
            })
            .collect();

        let mut content = Content::decode(&doc.get_page_content(page_id))?;

        // An inline image carries raw bytes that would not survive being parsed
        // and written back, so such a page is left alone.
        if content
            .operations
            .iter()
            .any(|operation| operation.operator == "BI")
        {
            return Err("the page contains an inline image, its text cannot be rewritten safely".into());
        }

        let mut font = None;
        let mut replaced = 0;

        for operation in &mut content.operations {
            match operation.operator.as_str() {
                // Tf selects the font the following text operators use.
                "Tf" => {
                    font = operation
                        .operands
                        .first()
                        .and_then(|operand| operand.as_name().ok())
                        .and_then(|name| fonts.get(name));
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    if let Some((dictionary, encoding)) = font {
                        let mut replace = |object: &mut Object| {
                            replace_in_string(object, doc, dictionary, encoding, search, replacement)
                        };
                        for operand in operation.operands.iter_mut() {
                            replaced += match operand {
                                Object::Array(items) => {
                                    items.iter_mut().map(&mut replace).sum::<Result<usize>>()?
                                }
                                operand => replace(operand)?,
                            };
                        }
                    }
                }
                _ => {}
            }
        }

        (content, replaced)
    };

    if replaced == 0 {
        return Ok(0);
    }

    // Writing the operations back must produce something that reads the same,
    // otherwise the page would come out deformed.
    let bytes = content.encode()?;
    let written = Content::decode(&bytes)?;
    if written.operations.len() != content.operations.len() {
        return Err("the page content could not be written back unchanged".into());
    }

    doc.change_page_content(page_id, bytes)?;
    Ok(replaced)
}

/// Replace inside one string operand, keeping its font and its format.
fn replace_in_string(
    object: &mut Object,
    doc: &Document,
    font: &Dictionary,
    encoding: &Encoding,
    search: &str,
    replacement: &str,
) -> Result<usize> {
    let Object::String(bytes, _) = object else {
        return Ok(0);
    };
    // A run this tool cannot read is a run it must not rewrite.
    let Ok(text) = Document::decode_text(encoding, bytes) else {
        return Ok(0);
    };
    if !text.contains(search) {
        return Ok(0);
    }

    let wanted = text.replace(search, replacement);
    let encoded = Document::encode_text(encoding, &wanted);

    // A font embedded as a subset carries only the glyphs the document already
    // uses; writing anything else would leave blanks, so refuse it instead.
    let missing = unavailable(doc, font, encoding, replacement);
    if !missing.is_empty() || !writes_back(encoding, &encoded, &wanted) {
        return Err(format!(
            "the font of \"{}\" has no {}. A font embedded in a PDF usually carries \
             only the characters the document already uses.",
            text.trim(),
            quoted(&missing)
        )
        .into());
    }

    *bytes = encoded;
    Ok(text.matches(search).count())
}

/// Does `bytes` read back as `wanted`?
fn writes_back(encoding: &Encoding, bytes: &[u8], wanted: &str) -> bool {
    Document::decode_text(encoding, bytes).is_ok_and(|read| read == wanted)
}

/// The characters of `text` the font cannot write.
pub fn unavailable(doc: &Document, font: &Dictionary, encoding: &Encoding, text: &str) -> Vec<char> {
    let mut missing: Vec<char> = text
        .chars()
        .filter(|c| !can_write(doc, font, encoding, *c))
        .collect();
    missing.dedup();
    missing
}

/// Can this font write that character?
fn can_write(doc: &Document, font: &Dictionary, encoding: &Encoding, character: char) -> bool {
    let one = character.to_string();
    let bytes = Document::encode_text(encoding, &one);

    // The encoding has to have a code for it, and that code has to read back.
    if bytes.is_empty() || !writes_back(encoding, &bytes, &one) {
        return false;
    }

    // And for a simple font, the font has to declare a glyph for that code.
    match bytes.as_slice() {
        [code] => declared_width(doc, font, *code).is_none_or(|width| width > 0.0),
        _ => true,
    }
}

/// The width a simple font declares for `code`: `None` when the font says
/// nothing (a standard font with no `Widths`), `Some(0)` when it declares no
/// glyph — which is how a subset marks the characters it left out.
fn declared_width(doc: &Document, font: &Dictionary, code: u8) -> Option<f32> {
    let widths = resolved(doc, font, b"Widths")?.as_array().ok()?;
    let first = resolved(doc, font, b"FirstChar")?.as_i64().ok()?;

    let index = i64::from(code) - first;
    if index < 0 || index as usize >= widths.len() {
        return Some(0.0); // outside the declared range: no glyph at all
    }
    let width = doc.dereference(&widths[index as usize]).ok()?.1;
    Some(width.as_float().unwrap_or(0.0))
}

/// A dictionary entry, following a reference if there is one.
fn resolved<'a>(doc: &'a Document, dictionary: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let object = dictionary.get(key).ok()?;
    Some(doc.dereference(object).ok()?.1)
}

/// `'a', 'b'` - or `"glyph for this text"` when we cannot name the characters.
fn quoted(characters: &[char]) -> String {
    match characters.is_empty() {
        true => "glyph for this text".to_string(),
        false => characters
            .iter()
            .map(|c| format!("'{c}'"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}
