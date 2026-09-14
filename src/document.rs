use std::collections::BTreeSet;
use std::sync::LazyLock;

use anyhow::{Result, bail, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::arguments::ReadArguments;
use crate::metadata::Metadata;

static SECTION_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+(?:\.\d+)*|[A-Za-z](?:\.\d+)*)[.:]?\s+").expect("valid section expression")
});
static CITED_IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:arxiv\s*:\s*|arxiv\.org/(?:abs|pdf|html)/)(\d{4}\.\d{4,5}(?:v\d+)?|[a-z][a-z.-]*/\d{7}(?:v\d+)?)",
    )
    .expect("valid citation expression")
});

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    Equation,
    Figure,
    Table,
    Reference,
    Code,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Block {
    pub kind: BlockKind,
    pub level: usize,
    pub text: String,
    pub markdown: String,
}

impl Block {
    pub fn heading(level: usize, text: String) -> Self {
        Self {
            kind: BlockKind::Heading,
            level,
            markdown: format!("{} {text}", "#".repeat(level)),
            text,
        }
    }

    pub fn content(kind: BlockKind, text: String, markdown: String) -> Self {
        Self {
            kind,
            level: 0,
            text,
            markdown,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Document {
    pub metadata: Metadata,
    pub origin: String,
    pub warnings: Vec<String>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Serialize)]
pub struct ChunkDescription {
    pub chunk: usize,
    pub characters: usize,
    pub starts_with: String,
}

impl Document {
    pub fn markdown(&self) -> String {
        let mut parts = vec![format!(
            "# {}\n\n**Authors:** {}\n\n**arXiv:** {}{}",
            self.metadata.title,
            self.metadata.authors.join(", "),
            self.metadata.id,
            self.metadata.version
        )];
        parts.extend(self.blocks.iter().map(|block| block.markdown.clone()));
        format!("{}\n", parts.join("\n\n"))
    }

    pub fn text(&self) -> String {
        let mut parts = vec![format!(
            "{}\n\nAuthors: {}\n\narXiv: {}{}",
            self.metadata.title,
            self.metadata.authors.join(", "),
            self.metadata.id,
            self.metadata.version
        )];
        parts.extend(self.blocks.iter().map(|block| block.text.clone()));
        format!("{}\n", parts.join("\n\n"))
    }

    pub fn toc(&self) -> String {
        self.blocks
            .iter()
            .filter(|block| block.kind == BlockKind::Heading)
            .map(|block| {
                format!(
                    "{}{}\n",
                    "  ".repeat(block.level.saturating_sub(2)),
                    block.text
                )
            })
            .collect()
    }

    fn section_start(&self, query: &str) -> Result<usize> {
        let query = query.trim().to_lowercase();
        ensure!(!query.is_empty(), "section selector must not be empty");
        let headings: Vec<_> = self
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.kind == BlockKind::Heading)
            .collect();
        let exact: Vec<_> = headings
            .iter()
            .filter(|(_, block)| {
                let title = block.text.to_lowercase();
                let number = SECTION_NUMBER.captures(&title);
                let name = SECTION_NUMBER.replace(&title, "");
                title == query || name == query || number.is_some_and(|capture| capture[1] == query)
            })
            .collect();
        if exact.len() == 1 {
            return Ok(exact[0].0);
        }
        let matching: Vec<_> = if exact.is_empty() {
            headings
                .iter()
                .filter(|(_, block)| block.text.to_lowercase().contains(&query))
                .collect()
        } else {
            exact
        };
        match matching.as_slice() {
            [entry] => Ok(entry.0),
            [] => bail!("section {query:?} was not found; use `arxiv toc` to list sections"),
            _ => {
                let candidates = matching
                    .iter()
                    .map(|(_, block)| block.text.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                bail!("section {query:?} is ambiguous: {candidates}")
            }
        }
    }

    fn section_end(&self, start: usize) -> usize {
        let level = self.blocks[start].level;
        self.blocks
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, block)| block.kind == BlockKind::Heading && block.level <= level)
            .map_or(self.blocks.len(), |(index, _)| index)
    }

    pub fn select(&self, arguments: &ReadArguments) -> Result<Self> {
        let mut document = self.clone();
        let start = arguments
            .section
            .as_ref()
            .or(arguments.from.as_ref())
            .map(|query| self.section_start(query))
            .transpose()?
            .unwrap_or(0);
        let end = if let Some(query) = arguments.section.as_ref().or(arguments.to.as_ref()) {
            let end_start = self.section_start(query)?;
            ensure!(start <= end_start, "section range is reversed");
            self.section_end(end_start)
        } else {
            self.blocks.len()
        };
        ensure!(start < end, "section range is empty");
        document.blocks = self.blocks[start..end].to_vec();
        if arguments.no_references {
            document.blocks.retain(|block| {
                block.kind != BlockKind::Reference
                    && !(block.kind == BlockKind::Heading && is_references(&block.text))
            });
        }
        if arguments.no_figures {
            document.blocks.retain(|block| block.kind != BlockKind::Figure);
        }
        Ok(document)
    }

    pub fn references(&self) -> String {
        self.blocks
            .iter()
            .filter(|block| block.kind == BlockKind::Reference)
            .map(|block| format!("{}\n", block.markdown))
            .collect()
    }

    pub fn reference_ids(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        CITED_IDENTIFIER
            .captures_iter(&self.references())
            .filter_map(|capture| {
                let identifier = capture[1].to_owned();
                seen.insert(identifier.clone()).then_some(identifier)
            })
            .collect()
    }

    pub fn chunks(&self) -> Vec<Vec<Block>> {
        let mut chunks = Vec::new();
        let mut current = Vec::new();
        let mut length = 0;
        for block in &self.blocks {
            let size = block.markdown.chars().count();
            if length + size > 12_000 && !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
                length = 0;
            }
            current.push(block.clone());
            length += size + 2;
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        chunks
    }
}

pub fn describe_chunks(chunks: &[Vec<Block>]) -> Vec<ChunkDescription> {
    chunks
        .iter()
        .enumerate()
        .map(|(index, blocks)| ChunkDescription {
            chunk: index + 1,
            characters: blocks
                .iter()
                .map(|block| block.markdown.chars().count() + 2)
                .sum(),
            starts_with: blocks
                .first()
                .map(|block| block.text.chars().take(80).collect())
                .unwrap_or_default(),
        })
        .collect()
}

pub fn is_references(title: &str) -> bool {
    let title = SECTION_NUMBER.replace(title, "").to_lowercase();
    matches!(title.trim(), "references" | "bibliography" | "literature cited")
}
