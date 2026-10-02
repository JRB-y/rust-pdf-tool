//! Page level edits: keeping, removing and rotating pages.

use std::path::Path;

use crate::tree::inherited;
use crate::util::{Result, load, parse_pages};

/// Keep only the selected pages, dropping everything else.
pub fn keep(file: &Path, spec: &str, output: &Path) -> Result<usize> {
    let mut doc = load(file)?;
    let total = doc.get_pages().len() as u32;
    let kept = parse_pages(spec, total)?;

    let dropped: Vec<u32> = (1..=total).filter(|page| !kept.contains(page)).collect();
    doc.delete_pages(&dropped);
    doc.prune_objects();
    doc.save(output)?;
    Ok(kept.len())
}

/// Remove the selected pages.
pub fn remove(file: &Path, spec: &str, output: &Path) -> Result<usize> {
    let mut doc = load(file)?;
    let total = doc.get_pages().len() as u32;
    let dropped = parse_pages(spec, total)?;

    if dropped.len() as u32 == total {
        return Err("that would remove every page".into());
    }

    doc.delete_pages(&dropped);
    doc.prune_objects();
    doc.save(output)?;
    Ok(dropped.len())
}

/// Rotate pages by `angle` degrees, clockwise, on top of their current rotation.
/// `spec` is `None` for the whole document.
pub fn rotate(file: &Path, angle: i64, spec: Option<&str>, output: &Path) -> Result<usize> {
    if angle % 90 != 0 {
        return Err("the angle must be a multiple of 90".into());
    }

    let mut doc = load(file)?;
    let pages = doc.get_pages();
    let selected = match spec {
        Some(spec) => parse_pages(spec, pages.len() as u32)?,
        None => pages.keys().copied().collect(),
    };

    for number in &selected {
        let Some(page_id) = pages.get(number) else {
            continue;
        };
        // The current rotation may come from a parent node rather than the page.
        let current = inherited(&doc, *page_id, b"Rotate")
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0);
        doc.get_dictionary_mut(*page_id)?
            .set("Rotate", (current + angle).rem_euclid(360));
    }

    doc.save(output)?;
    Ok(selected.len())
}
