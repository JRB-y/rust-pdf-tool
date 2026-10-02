//! Helpers for the page tree, where a page can inherit attributes from its
//! parent nodes instead of carrying them itself.

use lopdf::{Document, Object, ObjectId};

/// Attributes a page may inherit from its parent nodes.
pub const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Copy every attribute the page inherits from its ancestors onto the page, so
/// it stays self-contained when the page tree around it changes.
pub fn inline_inherited(doc: &mut Document, page_id: ObjectId) {
    for key in INHERITED {
        let already_set = doc
            .get_dictionary(page_id)
            .map(|page| page.has(key))
            .unwrap_or(true);
        if already_set {
            continue;
        }
        if let Some(value) = inherited(doc, page_id, key)
            && let Ok(page) = doc.get_dictionary_mut(page_id)
        {
            page.set(key.to_vec(), value);
        }
    }
}

/// Look `key` up on the page, then on its ancestors. The depth limit keeps a
/// damaged file with a looping `Parent` chain from spinning forever.
pub fn inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut node_id = Some(page_id);

    for _ in 0..32 {
        let dict = doc.get_dictionary(node_id?).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value.clone());
        }
        node_id = dict.get(b"Parent").ok()?.as_reference().ok();
    }
    None
}
