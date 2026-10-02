//! End-to-end checks against PDFs built on the fly.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use lopdf::{Document, Object, dictionary};

/// Build an argument list: `args!["keep", file, pages]`.
macro_rules! args {
    ($($arg:expr),* $(,)?) => { vec![$(OsString::from($arg)),*] };
}

/// Build a `pages`-page PDF where page attributes live on the parent node, so
/// the tests also cover inherited attributes.
fn sample(path: &Path, pages: usize, title: &str) {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let kids: Vec<Object> = (0..pages)
        .map(|_| {
            let contents = doc.add_object(lopdf::Stream::new(dictionary! {}, b"BT ET".to_vec()));
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => contents,
            })
            .into()
        })
        .collect();

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages as i64,
            // Inherited by every page.
            "Resources" => dictionary! {},
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );

    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let info_id = doc.add_object(dictionary! { "Title" => lopdf::text_string(title) });
    doc.trailer.set("Info", info_id);

    doc.save(path).unwrap();
}

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdftool-tests-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn page_count(path: &Path) -> usize {
    Document::load(path).unwrap().get_pages().len()
}

#[test]
fn merges_documents_and_keeps_page_boxes() {
    let dir = workspace("merge");
    let (a, b, out) = (dir.join("a.pdf"), dir.join("b.pdf"), dir.join("out.pdf"));
    sample(&a, 2, "First");
    sample(&b, 3, "Second");

    let total = pdftool_merge(&[a, b], &out);
    assert_eq!(total, 5);
    assert_eq!(page_count(&out), 5);

    // Flattening the tree must not lose the inherited page size.
    let doc = Document::load(&out).unwrap();
    for (_, page_id) in doc.get_pages() {
        let page = doc.get_dictionary(page_id).unwrap();
        assert!(page.has(b"MediaBox"), "page lost its MediaBox");
        assert!(page.has(b"Resources"), "page lost its Resources");
    }
}

#[test]
fn keeps_and_removes_pages() {
    let dir = workspace("pages");
    let (file, kept, trimmed) = (dir.join("in.pdf"), dir.join("kept.pdf"), dir.join("trimmed.pdf"));
    sample(&file, 5, "Pages");

    assert_eq!(pdftool_keep(&file, "1-2,5", &kept), 3);
    assert_eq!(page_count(&kept), 3);

    assert_eq!(pdftool_remove(&file, "2,4", &trimmed), 2);
    assert_eq!(page_count(&trimmed), 3);
}

#[test]
fn rotates_selected_pages_on_top_of_inherited_rotation() {
    let dir = workspace("rotate");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    sample(&file, 3, "Rotate");

    // Put a rotation on the shared parent so pages inherit 90 degrees.
    let mut doc = Document::load(&file).unwrap();
    let pages_id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(pages_id).unwrap().set("Rotate", 90);
    doc.save(&file).unwrap();

    assert_eq!(pdftool_rotate(&file, 90, Some("2"), &out), 1);

    let doc = Document::load(&out).unwrap();
    let pages = doc.get_pages();
    let rotation = |number: &u32| {
        doc.get_dictionary(pages[number])
            .unwrap()
            .get(b"Rotate")
            .and_then(|value| value.as_i64())
            .ok()
    };
    assert_eq!(rotation(&2), Some(180), "inherited 90 plus 90");
    assert_eq!(rotation(&1), None, "untouched page still inherits");
}

#[test]
fn reads_and_writes_metadata() {
    let dir = workspace("meta");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    sample(&file, 1, "Original");

    let before = Document::load_metadata(&file).unwrap();
    assert_eq!(before.title.as_deref(), Some("Original"));

    let title = "Nouveau titre é".to_string();
    let author = "Jürgen Böse".to_string();
    let subject = "Quarterly report".to_string();
    pdftool_set(
        &file,
        &[("title", &title), ("author", &author), ("subject", &subject)],
        &[],
        &out,
    );

    let after = Document::load_metadata(&out).unwrap();
    assert_eq!(
        after.title.as_deref(),
        Some("Nouveau titre é"),
        "non-ascii round trip"
    );
    assert_eq!(after.author.as_deref(), Some("Jürgen Böse"));
    assert_eq!(after.subject.as_deref(), Some("Quarterly report"));
    assert_eq!(after.page_count, 1);

    // A second pass, editing in place, drops a field again.
    pdftool(args!["set", &out, "--clear", "Subject"]);
    let cleared = Document::load_metadata(&out).unwrap();
    assert_eq!(cleared.subject, None, "cleared field");
    assert_eq!(
        cleared.author.as_deref(),
        Some("Jürgen Böse"),
        "other fields survive"
    );
}

// The crate is a binary, so the integration test drives it through its CLI.
fn pdftool(args: Vec<OsString>) -> String {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_pdftool"));
    let output = std::process::Command::new(binary).args(&args).output().unwrap();
    assert!(
        output.status.success(),
        "pdftool {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The commands report counts like "kept 3 page(s) -> ..."; pick that number up.
fn count_in(text: &str) -> usize {
    text.split(|c: char| !c.is_ascii_digit())
        .find_map(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("no count in output: {text}"))
}

fn pdftool_merge(inputs: &[PathBuf], output: &Path) -> usize {
    let mut args = args!["merge"];
    args.extend(inputs.iter().map(OsString::from));
    args.extend(args!["-o", output]);
    let reported = pdftool(args);
    // "merged 2 files into out.pdf (5 pages)"
    count_in(reported.rsplit_once('(').unwrap().1)
}

fn pdftool_keep(file: &Path, pages: &str, output: &Path) -> usize {
    count_in(&pdftool(args!["keep", file, pages, "-o", output]))
}

fn pdftool_remove(file: &Path, pages: &str, output: &Path) -> usize {
    count_in(&pdftool(args!["remove", file, pages, "-o", output]))
}

fn pdftool_rotate(file: &Path, angle: i64, pages: Option<&str>, output: &Path) -> usize {
    let mut args = args!["rotate", file, angle.to_string()];
    if let Some(pages) = pages {
        args.extend(args!["--pages", pages]);
    }
    args.extend(args!["-o", output]);
    count_in(&pdftool(args))
}

fn pdftool_set(file: &Path, changes: &[(&str, &String)], clear: &[String], output: &Path) {
    let mut args = args!["set", file];
    for (name, value) in changes {
        args.extend(args![format!("--{name}"), value.as_str()]);
    }
    for name in clear {
        args.extend(args!["--clear", name.as_str()]);
    }
    args.extend(args!["-o", output]);
    pdftool(args);
}
