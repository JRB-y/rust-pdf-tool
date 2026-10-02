//! pdftool - read and edit PDF metadata, merge PDFs and reorganize pages.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use pdftool::util::{Result, write_edit};
use pdftool::{merge, meta, pages};

#[derive(Parser)]
#[command(
    name = "pdftool",
    version,
    about = "Read and edit PDF metadata, merge PDFs and reorganize pages"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the metadata and page count of a PDF
    Info {
        /// PDF to read
        file: PathBuf,
    },

    /// Change metadata fields (writes the file in place unless --output is given)
    Set {
        /// PDF to edit
        file: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        keywords: Option<String>,
        #[arg(long)]
        creator: Option<String>,
        #[arg(long)]
        producer: Option<String>,
        /// Remove a field, e.g. --clear author (may be repeated)
        #[arg(long, value_name = "FIELD")]
        clear: Vec<String>,
        /// Write to this file instead of editing in place
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Join two or more PDFs into one, in the order given
    Merge {
        /// PDFs to join
        #[arg(num_args = 2.., required = true)]
        files: Vec<PathBuf>,
        /// File to write
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Keep only the given pages, e.g. "1-3,7"
    Keep {
        file: PathBuf,
        /// Pages to keep
        pages: String,
        /// Write to this file instead of editing in place
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Delete the given pages, e.g. "2,5-6"
    Remove {
        file: PathBuf,
        /// Pages to delete
        pages: String,
        /// Write to this file instead of editing in place
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Rotate pages clockwise by a multiple of 90 degrees
    Rotate {
        file: PathBuf,
        /// Degrees, e.g. 90, 180 or -90
        angle: i64,
        /// Pages to rotate, e.g. "1,4-6" (default: all)
        #[arg(long)]
        pages: Option<String>,
        /// Write to this file instead of editing in place
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Info { file } => meta::show(&file),

        Command::Set {
            file,
            title,
            author,
            subject,
            keywords,
            creator,
            producer,
            clear,
            output,
        } => {
            let values = [
                ("title", title),
                ("author", author),
                ("subject", subject),
                ("keywords", keywords),
                ("creator", creator),
                ("producer", producer),
            ];
            let changes: Vec<(&str, &String)> = values
                .iter()
                .filter_map(|(name, value)| value.as_ref().map(|value| (*name, value)))
                .collect();

            let clear: Vec<String> = clear.iter().map(|name| name.to_lowercase()).collect();
            for name in &clear {
                if !meta::FIELDS.contains(&name.as_str()) {
                    return Err(format!(
                        "unknown field '{name}', expected one of: {}",
                        meta::FIELDS.join(", ")
                    )
                    .into());
                }
                if changes.iter().any(|(set, _)| set == name) {
                    return Err(format!("'{name}' is both set and cleared").into());
                }
            }
            if changes.is_empty() && clear.is_empty() {
                return Err("nothing to change, pass a field such as --title or --clear".into());
            }

            let written = write_edit(&file, output, |target| {
                meta::update(&file, &changes, &clear, target)
            })?;
            println!(
                "{} field(s) updated, {} removed -> {}",
                changes.len(),
                clear.len(),
                written.display()
            );
            Ok(())
        }

        Command::Merge { files, output } => {
            let total = merge::merge(&files, &output)?;
            println!(
                "merged {} files into {} ({total} pages)",
                files.len(),
                output.display()
            );
            Ok(())
        }

        Command::Keep { file, pages, output } => {
            let mut kept = 0;
            let written = write_edit(&file, output, |target| {
                kept = pages::keep(&file, &pages, target)?;
                Ok(())
            })?;
            println!("kept {kept} page(s) -> {}", written.display());
            Ok(())
        }

        Command::Remove { file, pages, output } => {
            let mut removed = 0;
            let written = write_edit(&file, output, |target| {
                removed = pages::remove(&file, &pages, target)?;
                Ok(())
            })?;
            println!("removed {removed} page(s) -> {}", written.display());
            Ok(())
        }

        Command::Rotate {
            file,
            angle,
            pages,
            output,
        } => {
            let mut rotated = 0;
            let written = write_edit(&file, output, |target| {
                rotated = pages::rotate(&file, angle, pages.as_deref(), target)?;
                Ok(())
            })?;
            println!(
                "rotated {rotated} page(s) by {angle} degrees -> {}",
                written.display()
            );
            Ok(())
        }
    }
}
