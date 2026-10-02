//! Merging several PDFs into one.

use std::path::{Path, PathBuf};

use lopdf::{Document, Object, ObjectId};

use crate::tree::{INHERITED, inline_inherited};
use crate::util::{Result, load};

/// Append every page of `inputs[1..]` to `inputs[0]`, write the result and
/// report how many pages it has.
pub fn merge(inputs: &[PathBuf], output: &Path) -> Result<usize> {
    let mut merged = load(&inputs[0])?;
    let root_id = root_pages(&merged)?;
    let mut kids = take_pages(&mut merged, root_id);

    for path in &inputs[1..] {
        let mut doc = load(path)?;

        // Give the incoming objects ids that cannot clash with what we have.
        doc.renumber_objects_with(merged.max_id + 1);
        merged.max_id = doc.max_id;

        let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        for page_id in &page_ids {
            inline_inherited(&mut doc, *page_id);
        }

        merged.objects.extend(doc.objects);
        for page_id in page_ids {
            reparent(&mut merged, page_id, root_id);
            kids.push(Object::Reference(page_id));
        }
    }

    let count = kids.len() as i64;
    let root = merged.get_dictionary_mut(root_id)?;
    root.set("Kids", kids);
    root.set("Count", count);
    // Every page now carries its own attributes, so the root must not hand
    // down the first document's defaults to pages from the others.
    for key in INHERITED {
        root.remove(key);
    }

    // The other documents' catalogs and outlines are no longer referenced.
    merged.prune_objects();
    merged.save(output)?;
    Ok(count as usize)
}

/// Id of the document's root `Pages` node.
fn root_pages(doc: &Document) -> Result<ObjectId> {
    let id = doc.catalog()?.get(b"Pages")?.as_reference()?;
    Ok(id)
}

/// Flatten the first document's own pages under its root node and return them
/// as the start of the merged `Kids` list.
fn take_pages(doc: &mut Document, root_id: ObjectId) -> Vec<Object> {
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for page_id in &page_ids {
        inline_inherited(doc, *page_id);
        reparent(doc, *page_id, root_id);
    }
    page_ids.into_iter().map(Object::Reference).collect()
}

fn reparent(doc: &mut Document, page_id: ObjectId, parent_id: ObjectId) {
    if let Ok(page) = doc.get_dictionary_mut(page_id) {
        page.set("Parent", Object::Reference(parent_id));
    }
}
