use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use regex::Regex;

use crate::document::{Block, BlockKind, Document, is_references};
use crate::metadata::{Metadata, normalize};

static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+(?:\.\d+)*|[A-Z](?:\.\d+)*)\.?\s+([A-Z][^.!?]{2,100})$")
        .expect("valid PDF heading expression")
});

pub fn parse(path: &Path, metadata: Metadata) -> Result<Document> {
    let text = pdf_extract::extract_text(path).context("extracting text from the PDF")?;
    ensure!(
        text.split_whitespace().count() > 50,
        "PDF contains too little extractable text; scanned papers need OCR, which is not included"
    );
    let blocks = parse_text(&text, &metadata);
    Ok(Document {
        origin: format!("https://arxiv.org/pdf/{}", metadata.identifier()),
        metadata,
        warnings: vec![
            "PDF fallback: section boundaries are heuristic; equations, tables, figures, and reading order may be incomplete. Use `arxiv source` or inspect the PDF for fidelity."
                .to_owned(),
        ],
        blocks,
    })
}

pub fn parse_text(text: &str, metadata: &Metadata) -> Vec<Block> {
    let lines: Vec<_> = text.lines().map(str::trim).collect();
    let first_section = lines.iter().position(|line| HEADING.is_match(line));
    let mut blocks = vec![
        Block::heading(2, "Abstract".to_owned()),
        Block::content(
            BlockKind::Paragraph,
            metadata.abstract_text.clone(),
            metadata.abstract_text.clone(),
        ),
    ];
    let start = first_section.unwrap_or(0);
    if first_section.is_none() {
        blocks.push(Block::heading(2, "Extracted PDF text".to_owned()));
    }
    let mut paragraph = String::new();
    let mut in_references = false;
    for line in &lines[start..] {
        let line = normalize(line);
        if line.is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks, in_references);
            continue;
        }
        if line.chars().all(|character| character.is_ascii_digit()) || line.starts_with("arXiv:") {
            continue;
        }
        if is_references(&line) {
            flush_paragraph(&mut paragraph, &mut blocks, in_references);
            blocks.push(Block::heading(2, line));
            in_references = true;
        } else if let Some(captures) = HEADING.captures(&line) {
            flush_paragraph(&mut paragraph, &mut blocks, in_references);
            blocks.push(Block::heading(2 + captures[1].matches('.').count(), line));
            in_references = false;
        } else {
            if in_references && line.starts_with('[') {
                flush_paragraph(&mut paragraph, &mut blocks, true);
            }
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(&line);
        }
    }
    flush_paragraph(&mut paragraph, &mut blocks, in_references);
    blocks
}

fn flush_paragraph(paragraph: &mut String, blocks: &mut Vec<Block>, references: bool) {
    if !paragraph.is_empty() {
        let text = std::mem::take(paragraph);
        blocks.push(Block::content(
            if references { BlockKind::Reference } else { BlockKind::Paragraph },
            text.clone(),
            text,
        ));
    }
}
