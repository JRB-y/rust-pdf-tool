# pdftool

A small tool for everyday PDF work: read and change metadata, merge files, and
keep, delete or rotate pages. It comes as a window and as a command line, both
on top of the same library, and is built on
[lopdf](https://crates.io/crates/lopdf) — nothing to install besides Rust.

```
cargo run                  # opens the window
cargo build --release      # target/release/pdftool-gui and target/release/pdftool
cargo install --path .     # installs both
```

## The window (`pdftool-gui`)

```
+----------------------------------------------------------------------+
| Ajouter des PDF… | Vider la liste | Métadonnées | Texte | Pages | …   |
+---------------------+------------------------------------------------+
| Fichiers            |  the action for the selected file              |
|  ↑ ↓ ✖  cover.pdf   |                                                |
|  ↑ ↓ ✖  body.pdf    |                                                |
+---------------------+------------------------------------------------+
| [x] Remplacer le fichier d'origine  |  status of the last action      |
+----------------------------------------------------------------------+
```

- Drop PDFs onto the window, or add them with the button. The list order is the
  merge order; `↑`/`↓` reorder it, `✖` removes an entry.
- **Métadonnées** shows the selected file's fields in text boxes. Edit them and
  save; emptying a box removes that field from the document.
- **Texte** shows the document's text page by page, and replaces or deletes a
  piece of it. See *Editing text* below for what a PDF allows.
- **Pages** takes a selection such as `1-3,7` and can keep, delete or rotate it.
- **Fusionner** joins every file in the list, in order.
- The checkbox at the bottom decides where output goes: ticked, the original is
  rewritten in place; unticked (the default), each action asks for a file name.

The window's labels are French; messages coming from the engine are English.

## The command line (`pdftool`)

```
pdftool info     <file>                         show metadata and page count
pdftool set      <file> [fields] [-o out.pdf]   change metadata
pdftool merge    <file> <file>... -o out.pdf    join PDFs, in the order given
pdftool keep     <file> <pages>  [-o out.pdf]   keep only these pages
pdftool remove   <file> <pages>  [-o out.pdf]   delete these pages
pdftool rotate   <file> <angle>  [-o out.pdf]   rotate clockwise
pdftool text     <file>                         print the text, page by page
pdftool replace  <file> --from X [--to Y]       replace text (no --to deletes it)
```

Every editing command writes the file **in place** unless you pass `-o`/`--output`.
In-place edits go to a temporary file first and are renamed over the original
only on success, so a failure never leaves you with a half-written PDF.

Page selections are 1-based and accept lists and ranges: `3`, `1-4`, `2,5-7`,
`8-` (to the end), `-3` (from the start).

## Examples

```sh
# What is in this file?
pdftool info report.pdf

# Set metadata; fields are title, author, subject, keywords, creator, producer
pdftool set report.pdf --title "Q3 Report" --author "J. Youssef" --keywords "finance,2026"

# Remove a field
pdftool set report.pdf --clear producer

# Write the change to a new file instead of editing in place
pdftool set report.pdf --title "Q3 Report" -o report-tagged.pdf

# Replace a piece of text, keeping its font, size and position
pdftool replace report.pdf --from "Q3 2025" --to "Q4 2025"

# Delete a piece of text (leave out --to)
pdftool replace report.pdf --from "DRAFT" -o final.pdf

# Merge; pages are appended in the order the files are listed
pdftool merge cover.pdf body.pdf appendix.pdf -o complete.pdf

# Split out a chapter, drop a page, turn a landscape scan upright
pdftool keep complete.pdf 4-12 -o chapter.pdf
pdftool remove complete.pdf 2
pdftool rotate scan.pdf -90 --pages 3,5-6
```

## Editing text

A PDF holds no flowing text: each run of glyphs sits at a fixed position, in a
font that producers almost always embed as a *subset* — only the glyphs the
document actually uses. So a replacement here:

- **keeps the font, size and position** of the text it replaces, and leaves the
  rest of the page byte for byte as it was;
- **cannot reflow**: a longer replacement extends to the right and may run into
  whatever follows it on that line. Equal length or shorter is always safe, and
  deleting text never moves anything;
- **is refused when the font lacks a glyph.** Writing `é` with a font that never
  used one would silently leave a blank, so the tool checks the font's own
  `FirstChar`/`Widths` tables as well as its encoding, names the characters it
  cannot draw, and writes nothing. Use only characters the document already
  contains, or delete the text instead;
- **matches inside one text run.** A word split by kerning (`[(Wor) -20 (ld)]`)
  cannot be matched — the tool says so rather than half-replacing it; try a
  shorter piece.

Verified by rendering before and after with Ghostscript: replacing `line 42` with
`line 24` in a four-page document changed 122 pixels of an 850×1100 render, all
inside a 20×13 box, and the other three pages came out pixel-identical.

## Notes

- Merging flattens the page tree and copies inherited attributes (page size,
  resources, rotation) onto each page, so pages keep their original look.
  Bookmarks and outlines of the *added* files are dropped; the first file's are kept.
- Metadata is written to the document information dictionary. Non-ASCII text is
  stored as UTF-16BE, as the PDF specification requires.
- Encrypted (password protected) PDFs are not supported.

## Layout

| File | Contents |
| --- | --- |
| `src/lib.rs` | the library both front ends use |
| `src/bin/pdftool-gui/` | the window: `main.rs` opens it, `app.rs` is the interface |
| `src/main.rs` | the command line interface and dispatch |
| `src/meta.rs` | reading and writing the Info dictionary |
| `src/merge.rs` | merging documents |
| `src/pages.rs` | keep, remove, rotate |
| `src/text.rs` | reading and replacing text, with the glyph checks |
| `src/tree.rs` | page tree helpers (inherited attributes) |
| `src/util.rs` | page range parsing, date formatting, loading |

`cargo test` builds sample PDFs on the fly and drives the binary through its CLI.
