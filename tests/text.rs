//! Text replacement, including the cases that must be refused.

use std::fs;
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

/// A one-page PDF saying `line` in font `font`.
fn sample(path: &Path, line: &str, font: lopdf::Dictionary) {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(font);
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(line)]),
            Operation::new("ET", vec![]),
        ],
    };
    let contents_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => contents_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

/// One of the standard 14 fonts: no `Widths`, every Latin glyph available.
fn standard_font() -> lopdf::Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }
}

/// A font embedded as a subset: it declares a glyph only for the few codes it
/// kept, exactly as a real producer does (width 0 or outside FirstChar/LastChar).
fn subset_font(available: &str) -> lopdf::Dictionary {
    let (first, last) = (32u8, 126u8);
    let widths: Vec<Object> = (first..=last)
        .map(|code| match available.contains(code as char) {
            true => 600.into(),
            false => 0.into(),
        })
        .collect();

    dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "AAAAAA+Subset",
        "Encoding" => "WinAnsiEncoding",
        "FirstChar" => first as i64,
        "LastChar" => last as i64,
        "Widths" => widths,
    }
}

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdftool-text-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_of(path: &Path) -> String {
    Document::load(path).unwrap().extract_text(&[1]).unwrap()
}

#[test]
fn replaces_text_in_a_standard_font() {
    let dir = workspace("standard");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    sample(&file, "Hello World!", standard_font());

    let report = pdftool::text::replace(&file, "World", "Montpellier", &out).unwrap();
    assert_eq!(report.replaced, 1);
    assert_eq!(report.pages, vec![1]);
    assert!(
        text_of(&out).contains("Hello Montpellier!"),
        "{:?}",
        text_of(&out)
    );
}

#[test]
fn deletes_text_when_the_replacement_is_empty() {
    let dir = workspace("delete");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    sample(&file, "Keep this. Drop that.", standard_font());

    let report = pdftool::text::replace(&file, " Drop that.", "", &out).unwrap();
    assert_eq!(report.replaced, 1);
    let left = text_of(&out);
    assert!(left.contains("Keep this."), "{left:?}");
    assert!(!left.contains("Drop"), "{left:?}");
}

#[test]
fn refuses_characters_the_embedded_font_does_not_have() {
    let dir = workspace("subset");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    // The subset carries only the letters of the line itself.
    sample(&file, "Hello real world", subset_font("Helo raw d"));

    // 'S' and 'u' are inside FirstChar..LastChar but have width 0,
    // 'é' is outside the declared range entirely.
    for (replacement, expected) in [("Salut", "'S'"), ("Héllo", "'é'")] {
        let error = pdftool::text::replace(&file, "Hello", replacement, &out)
            .expect_err("a missing glyph must be refused, not written blank")
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
    assert!(
        !out.exists(),
        "nothing may be written when the text cannot be drawn"
    );

    // Letters the font does have are fine.
    let report = pdftool::text::replace(&file, "Hello", "Hallo", &out).unwrap();
    assert_eq!(report.replaced, 1);
}

#[test]
fn reports_text_it_cannot_find() {
    let dir = workspace("missing");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    sample(&file, "Hello World!", standard_font());

    let error = pdftool::text::replace(&file, "Goodbye", "Hi", &out)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not found"), "{error}");
    assert!(!out.exists(), "a failed replacement must not write a file");
}

#[test]
fn lists_the_text_page_by_page() {
    let dir = workspace("pages");
    let file = dir.join("in.pdf");
    sample(&file, "Hello World!", standard_font());

    let pages = pdftool::text::pages(&file).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].0, 1);
    assert!(pages[0].1.contains("Hello World!"), "{:?}", pages[0].1);
}

/// The promise this feature makes is that the rest of the page is untouched.
/// Rendering both files and comparing the pixels is the only honest check of
/// that, so it runs whenever Ghostscript is available.
#[test]
fn renders_identically_outside_the_replaced_text() {
    if std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipped: ghostscript is not installed");
        return;
    }

    let dir = workspace("render");
    let (file, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
    sample(&file, "Hello World!", standard_font());
    pdftool::text::replace(&file, "World", "Monde", &out).unwrap();

    let (width, height, before) = render(&file, &dir, "before");
    let (after_width, after_height, after) = render(&out, &dir, "after");
    assert_eq!(
        (width, height),
        (after_width, after_height),
        "the page size changed"
    );

    let changed: Vec<usize> = before
        .iter()
        .zip(&after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, _)| index)
        .collect();

    assert!(!changed.is_empty(), "the text should have changed");
    let rows: Vec<usize> = changed.iter().map(|index| index / width).collect();
    let band = rows.iter().max().unwrap() - rows.iter().min().unwrap();
    assert!(
        band <= 60,
        "the change spreads over {band} rows, more than one line of text"
    );
    let ratio = changed.len() as f32 / before.len() as f32;
    assert!(ratio < 0.02, "{:.1}% of the page changed", ratio * 100.0);
}

/// Render page 1 to greyscale pixels with Ghostscript.
fn render(file: &Path, dir: &Path, name: &str) -> (usize, usize, Vec<u8>) {
    let image = dir.join(format!("{name}.pgm"));
    let status = std::process::Command::new("gs")
        .args(["-q", "-dLastPage=1", "-sDEVICE=pgmraw", "-r100", "-o"])
        .arg(&image)
        .arg(file)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "ghostscript could not render {}",
        file.display()
    );

    let data = fs::read(&image).unwrap();
    let mut numbers = Vec::new();
    let mut at = 2; // past the "P5" magic
    while numbers.len() < 3 {
        while data[at].is_ascii_whitespace() {
            at += 1;
        }
        if data[at] == b'#' {
            // a comment line, Ghostscript writes one
            while data[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        let start = at;
        while !data[at].is_ascii_whitespace() {
            at += 1;
        }
        numbers.push(
            String::from_utf8_lossy(&data[start..at])
                .parse::<usize>()
                .unwrap(),
        );
    }
    let (width, height) = (numbers[0], numbers[1]);
    (width, height, data[at + 1..at + 1 + width * height].to_vec())
}
