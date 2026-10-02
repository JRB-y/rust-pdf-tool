# pdftool

A small command line tool for everyday PDF work: read and change metadata, merge
files, and keep, delete or rotate pages. Built on [lopdf](https://crates.io/crates/lopdf),
so there is nothing to install besides Rust.

```
cargo build --release      # binary at target/release/pdftool
cargo install --path .     # or install it as `pdftool`
```

## Commands

```
pdftool info     <file>                         show metadata and page count
pdftool set      <file> [fields] [-o out.pdf]   change metadata
pdftool merge    <file> <file>... -o out.pdf    join PDFs, in the order given
pdftool keep     <file> <pages>  [-o out.pdf]   keep only these pages
pdftool remove   <file> <pages>  [-o out.pdf]   delete these pages
pdftool rotate   <file> <angle>  [-o out.pdf]   rotate clockwise
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

# Merge; pages are appended in the order the files are listed
pdftool merge cover.pdf body.pdf appendix.pdf -o complete.pdf

# Split out a chapter, drop a page, turn a landscape scan upright
pdftool keep complete.pdf 4-12 -o chapter.pdf
pdftool remove complete.pdf 2
pdftool rotate scan.pdf -90 --pages 3,5-6
```

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
| `src/main.rs` | the command line interface and dispatch |
| `src/meta.rs` | reading and writing the Info dictionary |
| `src/merge.rs` | merging documents |
| `src/pages.rs` | keep, remove, rotate |
| `src/tree.rs` | page tree helpers (inherited attributes) |
| `src/util.rs` | page range parsing, date formatting, loading |

`cargo test` builds sample PDFs on the fly and drives the binary through its CLI.
