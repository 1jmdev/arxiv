use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Read arXiv papers as structured Markdown for research agents"
)]
pub struct Arguments {
    /// Override the cache directory (also configurable with ARXIV_CACHE_DIR).
    #[arg(long, global = true)]
    pub cache_dir: Option<PathBuf>,
    /// Refresh cached web pages and downloads.
    #[arg(long, global = true, conflicts_with = "offline")]
    pub refresh: bool,
    /// Use cached resources only; never make a network request.
    #[arg(long, global = true)]
    pub offline: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Read the paper, preferring cleaned HTML and falling back to PDF text.
    Read(ReadArguments),
    /// Print only the abstract.
    Abstract { id: String },
    /// Print bibliographic metadata from the abstract web page.
    Metadata {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// List submission versions and their UTC dates.
    Versions { id: String },
    /// Print the paper's section hierarchy.
    Toc { id: String },
    /// List figure captions and their image or graph URLs.
    Figures {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Cache a figure's panels and print paths; SVG graphs are rendered to PNG.
    Figure {
        id: String,
        figure: String,
        /// Keep original SVG files instead of rendering PNG copies.
        #[arg(long)]
        original: bool,
    },
    /// Read a section by number or case-insensitive title.
    Section { id: String, section: String },
    /// Search arXiv web pages, without using the metadata API.
    Search(SearchArguments),
    /// Print the bibliography, or just explicitly cited arXiv identifiers.
    References {
        id: String,
        #[arg(long)]
        ids: bool,
    },
    /// Print the cached source path, list files, or read one text file.
    Source {
        id: String,
        #[arg(long, conflicts_with = "file")]
        files: bool,
        #[arg(long)]
        file: Option<String>,
    },
    /// Download the original source payload without extracting it.
    DownloadSource { id: String, path: PathBuf },
    /// Download the PDF, or print its local cached path.
    Pdf { id: String, path: Option<PathBuf> },
    /// Download the original paper HTML, or print its cached path.
    Html { id: String, path: Option<PathBuf> },
    /// Print a plain bibliographic citation.
    Cite { id: String },
    /// Generate a BibTeX entry from the paper's metadata.
    Bibtex { id: String },
}

#[derive(Clone, Debug, Default, Args)]
pub struct ReadArguments {
    pub id: String,
    #[arg(long, conflicts_with_all = ["from", "to"])]
    pub section: Option<String>,
    /// First section in an inclusive range.
    #[arg(long)]
    pub from: Option<String>,
    /// Last section in an inclusive range, including its subsections.
    #[arg(long)]
    pub to: Option<String>,
    /// Read a one-based chunk of approximately 12,000 characters.
    #[arg(long, value_parser = clap::value_parser!(usize), conflicts_with = "chunks")]
    pub chunk: Option<usize>,
    /// List chunk numbers, character counts, and starting headings.
    #[arg(long)]
    pub chunks: bool,
    /// Include the bibliography (the default).
    #[arg(long, conflicts_with = "no_references")]
    pub references: bool,
    #[arg(long)]
    pub no_references: bool,
    /// Include figures and captions (the default).
    #[arg(long, conflicts_with = "no_figures")]
    pub figures: bool,
    #[arg(long)]
    pub no_figures: bool,
    #[arg(long, value_enum, default_value = "markdown")]
    pub format: OutputFormat,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Markdown,
    Text,
    Json,
}

#[derive(Debug, Args)]
pub struct SearchArguments {
    pub query: String,
    #[arg(long, conflicts_with = "json")]
    pub ids: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub category: Option<String>,
    #[arg(long)]
    pub author: Option<String>,
    /// Earliest submission date, in YYYY-MM-DD format.
    #[arg(long)]
    pub since: Option<chrono::NaiveDate>,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=1000))]
    pub limit: u16,
}
