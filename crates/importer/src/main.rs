//! `magnetite-import`: convert another browser's sidebar, tabs and bookmarks into
//! magnetite JSON or bookmarks HTML.

use std::io::Write as _;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(
    name = "magnetite-import",
    about = "Import from Arc, Chromium-based browsers, Firefox or Zen. Reads only your local files; nothing leaves your machine."
)]
struct Cli {
    /// Path to an Arc StorableSidebar.json. Defaults to the Arc location on this platform.
    #[arg(short, long, conflicts_with = "from")]
    input: Option<PathBuf>,

    /// Import from a browser on this computer instead, by key (see --list).
    #[arg(long)]
    from: Option<String>,

    /// List the browsers on this computer that can be imported from.
    #[arg(long)]
    list: bool,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Json)]
    format: Format,

    /// Output file. Writes to stdout when omitted.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Print a summary (spaces, tab counts) to stderr.
    #[arg(long)]
    summary: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// magnetite-sidebar JSON (our native format)
    Json,
    /// Netscape bookmarks HTML, importable by any browser
    Html,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.list {
        for b in magnetite_importer::installed() {
            println!("{:<10} {}  ({})", b.key, b.name, b.path.display());
        }
        return Ok(());
    }
    let snapshot = if let Some(key) = &cli.from {
        let browser = magnetite_importer::installed()
            .into_iter()
            .find(|b| b.key == key)
            .with_context(|| {
                format!("no importable browser \"{key}\" on this computer; see --list")
            })?;
        magnetite_importer::import(&browser)?
    } else {
        let input = match cli.input {
            Some(p) => p,
            None => magnetite_importer::default_sidebar_path()
                .context("no default sidebar location on this platform; pass --input")?,
        };
        if !input.exists() {
            bail!("{} does not exist", input.display());
        }
        magnetite_importer::import_file(&input)?
    };

    if cli.summary {
        let favs: usize = snapshot.favorites.iter().map(|f| f.items.len()).sum();
        eprintln!("{} spaces, {favs} favorites", snapshot.spaces.len());
        for s in &snapshot.spaces {
            let pinned = s
                .pinned
                .iter()
                .flat_map(magnetite_core::SidebarNode::tabs)
                .count();
            // The title comes from the file: no control characters (escape
            // sequences) to the terminal.
            let title: String = s.title.chars().filter(|c| !c.is_control()).collect();
            eprintln!(
                "  {:<24} pinned {:>3}  today {:>3}",
                title,
                pinned,
                s.today.len()
            );
        }
    }

    let text = match cli.format {
        Format::Json => serde_json::to_string_pretty(&snapshot)?,
        Format::Html => magnetite_importer::export::bookmarks_html(&snapshot),
    };
    match cli.output {
        Some(path) => std::fs::write(&path, text)
            .with_context(|| format!("cannot write {}", path.display()))?,
        None => match writeln!(std::io::stdout(), "{text}") {
            // A reader that stops early (`| head`) has all it wanted.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
            written => written.context("cannot write to standard output")?,
        },
    }
    Ok(())
}
