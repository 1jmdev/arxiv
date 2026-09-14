use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};

use crate::document::Document;
use crate::html_document;
use crate::metadata::{self, Metadata};
use crate::pdf_document;
use crate::web_client::WebClient;

pub fn download(client: &WebClient, metadata: &Metadata, format: &str) -> Result<PathBuf> {
    let identifier = metadata.identifier();
    let (endpoint, filename) = match format {
        "pdf" => ("pdf", "paper.pdf"),
        "html" => ("html", "paper.html"),
        "source" => ("src", "source.archive"),
        _ => unreachable!("internal download format"),
    };
    let relative = Path::new(&identifier.cache_key()).join(filename);
    client.resource(
        &format!("https://arxiv.org/{endpoint}/{identifier}"),
        &relative,
        None,
        |bytes| {
            ensure!(!bytes.is_empty(), "arXiv returned an empty resource");
            match format {
                "pdf" => ensure!(bytes.starts_with(b"%PDF-"), "arXiv did not return a PDF"),
                "html" => {
                    html_document::parse(std::str::from_utf8(bytes)?, metadata.clone())?;
                }
                "source" => {
                    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
                    let prefix = prefix.trim_start().to_lowercase();
                    ensure!(
                        !prefix.starts_with("<!doctype html") && !prefix.starts_with("<html"),
                        "arXiv returned a web page instead of source"
                    );
                }
                _ => unreachable!(),
            }
            Ok(())
        },
    )
}

pub fn load(client: &WebClient, identifier: &crate::identifier::Identifier) -> Result<Document> {
    let metadata = metadata::load(client, identifier)?;
    match download(client, &metadata, "html") {
        Ok(path) => html_document::parse(&fs::read_to_string(path)?, metadata),
        Err(error) => {
            eprintln!("arxiv: HTML unavailable ({error:#}); attempting PDF fallback");
            let path = download(client, &metadata, "pdf")?;
            pdf_document::parse(&path, metadata)
        }
    }
}
