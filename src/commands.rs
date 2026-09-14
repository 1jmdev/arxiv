use std::fs;
use std::path::Path;

use anyhow::{Result, ensure};

use crate::arguments::{Arguments, Command, OutputFormat, ReadArguments};
use crate::citation;
use crate::document::describe_chunks;
use crate::figures;
use crate::metadata;
use crate::paper;
use crate::search;
use crate::source_archive;
use crate::web_client::WebClient;

pub fn execute(arguments: Arguments) -> Result<String> {
    let client = WebClient::new(arguments.cache_dir, arguments.refresh, arguments.offline)?;
    match arguments.command {
        Command::Read(arguments) => read(&client, arguments),
        Command::Section { id, section } => read(
            &client,
            ReadArguments {
                id,
                section: Some(section),
                ..ReadArguments::default()
            },
        ),
        Command::Abstract { id } => Ok(format!(
            "{}\n",
            metadata::load(&client, &id.parse()?)?.abstract_text
        )),
        Command::Metadata { id, json } => {
            let metadata = metadata::load(&client, &id.parse()?)?;
            if json {
                json_output(&metadata)
            } else {
                Ok(metadata.display())
            }
        }
        Command::Versions { id } => {
            let mut identifier = id.parse::<crate::identifier::Identifier>()?;
            identifier.version = None;
            let metadata = metadata::load(&client, &identifier)?;
            Ok(metadata
                .versions
                .iter()
                .map(|entry| format!("{}  {}\n", entry.version, entry.submitted))
                .collect())
        }
        Command::Toc { id } => {
            let document = paper::load(&client, &id.parse()?)?;
            report_warnings(&document.warnings);
            Ok(document.toc())
        }
        Command::Figures { id, json } => {
            let document = paper::load(&client, &id.parse()?)?;
            report_warnings(&document.warnings);
            let figures: Vec<_> = document
                .blocks
                .iter()
                .filter_map(|block| block.figure.clone())
                .collect();
            if json {
                json_output(&figures)
            } else {
                Ok(figures
                    .iter()
                    .map(|figure| format!("{}\n\n", figure.markdown()))
                    .collect())
            }
        }
        Command::Figure {
            id,
            figure,
            original,
        } => {
            let document = paper::load(&client, &id.parse()?)?;
            let figures: Vec<_> = document
                .blocks
                .iter()
                .filter_map(|block| block.figure.clone())
                .collect();
            let selected = figures::select(&figures, &figure)?;
            let paths = figures::download(&client, &document.metadata, selected, original)?;
            Ok(paths
                .iter()
                .map(|path| format!("{}\n", path.display()))
                .collect())
        }
        Command::References { id, ids } => {
            let document = paper::load(&client, &id.parse()?)?;
            report_warnings(&document.warnings);
            if ids {
                Ok(document
                    .reference_ids()
                    .iter()
                    .map(|id| format!("{id}\n"))
                    .collect())
            } else {
                Ok(document.references())
            }
        }
        Command::Search(arguments) => {
            let results = search::execute(&client, &arguments)?;
            if arguments.json {
                json_output(&results)
            } else if arguments.ids {
                Ok(results
                    .iter()
                    .map(|paper| format!("{}\n", paper.id))
                    .collect())
            } else {
                Ok(results
                    .iter()
                    .map(|paper| {
                        format!(
                            "{}  {}\n    {}\n    {}\n\n",
                            paper.id,
                            paper.title,
                            paper.authors.join(", "),
                            paper.abstract_text
                        )
                    })
                    .collect())
            }
        }
        Command::Source { id, files, file } => {
            let metadata = metadata::load(&client, &id.parse()?)?;
            let path = paper::download(&client, &metadata, "source")?;
            if !files && file.is_none() {
                return Ok(format!("{}\n", fs::canonicalize(path)?.display()));
            }
            let entries = source_archive::read(&path)?;
            if let Some(file) = file {
                source_archive::file_text(&entries, &file)
            } else {
                Ok(entries
                    .iter()
                    .map(|entry| format!("{}\n", entry.name))
                    .collect())
            }
        }
        Command::DownloadSource { id, path } => download(&client, &id, "source", Some(&path)),
        Command::Pdf { id, path } => download(&client, &id, "pdf", path.as_deref()),
        Command::Html { id, path } => download(&client, &id, "html", path.as_deref()),
        Command::Cite { id } => Ok(citation::citation(&metadata::load(&client, &id.parse()?)?)),
        Command::Bibtex { id } => Ok(citation::bibtex(&metadata::load(&client, &id.parse()?)?)),
    }
}

fn report_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("arxiv: {warning}");
    }
}

fn json_output(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn read(client: &WebClient, arguments: ReadArguments) -> Result<String> {
    let document = paper::load(client, &arguments.id.parse()?)?;
    report_warnings(&document.warnings);
    let mut document = document.select(&arguments)?;
    if arguments.chunks || arguments.chunk.is_some() {
        let chunks = document.chunks();
        if arguments.chunks {
            let descriptions = describe_chunks(&chunks);
            if matches!(arguments.format, OutputFormat::Json) {
                return json_output(&descriptions);
            }
            return Ok(descriptions
                .iter()
                .map(|chunk| {
                    format!(
                        "{}  {} characters  {}\n",
                        chunk.chunk, chunk.characters, chunk.starts_with
                    )
                })
                .collect());
        }
        let index = arguments.chunk.expect("chunk selection");
        ensure!(
            index > 0 && index <= chunks.len(),
            "chunk must be between 1 and {}",
            chunks.len()
        );
        document.blocks = chunks[index - 1].clone();
    }
    match arguments.format {
        OutputFormat::Markdown => Ok(document.markdown()),
        OutputFormat::Text => Ok(document.text()),
        OutputFormat::Json => json_output(&document),
    }
}

fn download(
    client: &WebClient,
    id: &str,
    format: &str,
    destination: Option<&Path>,
) -> Result<String> {
    if let Some(destination) = destination {
        ensure!(
            !destination.exists(),
            "destination already exists: {}",
            destination.display()
        );
    }
    let metadata = metadata::load(client, &id.parse()?)?;
    let cached = paper::download(client, &metadata, format)?;
    let path = if let Some(destination) = destination {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        std::io::copy(&mut fs::File::open(&cached)?, &mut temporary)?;
        temporary.persist_noclobber(destination)?;
        destination.to_path_buf()
    } else {
        cached
    };
    Ok(format!("{}\n", fs::canonicalize(path)?.display()))
}
