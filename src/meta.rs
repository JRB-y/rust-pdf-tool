//! Reading and updating the document information dictionary.

use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId, PdfMetadata, decode_text_string, text_string};

use crate::util::{Result, load, pretty_date};

/// The Info fields this tool knows about, in display order.
pub const FIELDS: [&str; 6] = ["title", "author", "subject", "keywords", "creator", "producer"];

/// Read metadata and page count without loading the whole document.
pub fn read(file: &Path) -> Result<PdfMetadata> {
    Document::load_metadata(file).map_err(|error| format!("could not read {}: {error}", file.display()).into())
}

/// Print what `read` found.
pub fn show(file: &Path) -> Result<()> {
    let meta = read(file)?;

    println!("File:     {}", file.display());
    println!("Pages:    {}", meta.page_count);
    println!("Version:  PDF {}", meta.version);
    if meta.encrypted {
        println!("Note:     document is encrypted");
    }
    println!();

    let rows = [
        ("Title", meta.title),
        ("Author", meta.author),
        ("Subject", meta.subject),
        ("Keywords", meta.keywords),
        ("Creator", meta.creator),
        ("Producer", meta.producer),
        ("Created", meta.creation_date.as_deref().map(pretty_date)),
        ("Modified", meta.modification_date.as_deref().map(pretty_date)),
    ];
    for (label, value) in rows {
        println!("{label:<10}{}", value.unwrap_or_else(|| "-".into()));
    }

    let mut custom: Vec<_> = meta.custom.iter().collect();
    custom.sort_by_key(|(key, _)| key.to_vec());
    if !custom.is_empty() {
        println!("\nOther entries:");
        for (key, value) in custom {
            let name = String::from_utf8_lossy(key);
            let shown = decode_text_string(value).unwrap_or_else(|_| format!("{value:?}"));
            println!("{:<10}{shown}", name);
        }
    }

    Ok(())
}

/// Apply the `field = value` updates in `changes` and remove the fields in
/// `clear`, then write the document to `output`.
pub fn update(file: &Path, changes: &[(&str, &String)], clear: &[String], output: &Path) -> Result<()> {
    let mut doc = load(file)?;
    let info_id = info_dictionary(&mut doc)?;
    let info = doc.get_dictionary_mut(info_id)?;

    for name in clear {
        info.remove(pdf_key(name).as_bytes());
    }
    for (name, value) in changes {
        info.set(pdf_key(name), text_string(value));
    }

    doc.save(output)?;
    Ok(())
}

/// Capitalise a field name the way the PDF spec spells it: `title` -> `Title`.
fn pdf_key(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The id of the Info dictionary, creating one if the document has none.
fn info_dictionary(doc: &mut Document) -> Result<ObjectId> {
    match doc.trailer.get(b"Info") {
        // The usual case: Info is an indirect object.
        Ok(Object::Reference(id)) if doc.has_object(*id) => Ok(*id),
        // Rare, but some files inline it; move it into an object so we can edit it.
        Ok(Object::Dictionary(dict)) => {
            let dict = dict.clone();
            Ok(attach_info(doc, dict))
        }
        _ => Ok(attach_info(doc, Dictionary::new())),
    }
}

fn attach_info(doc: &mut Document, dict: Dictionary) -> ObjectId {
    let id = doc.add_object(dict);
    doc.trailer.set("Info", Object::Reference(id));
    id
}
