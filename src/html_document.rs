use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use regex::Regex;
use scraper::{ElementRef, Html};

use crate::document::{Block, BlockKind, Document, is_references};
use crate::metadata::{Metadata, element_text, normalize, selector};

static MARKDOWN_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\]\(([^)\s]+)\)").expect("valid Markdown link expression")
});

pub fn parse(html: &str, metadata: Metadata) -> Result<Document> {
    let document = Html::parse_document(html);
    let root = document
        .select(&selector("article.ltx_document, .ltx_document, article"))
        .next()
        .context("HTML does not contain a research paper article")?;
    let mut blocks = Vec::new();
    collect(root, &mut blocks, false);
    let origin = format!("https://arxiv.org/html/{}", metadata.identifier());
    let base_url = reqwest::Url::parse(&origin)?;
    for block in &mut blocks {
        block.markdown = MARKDOWN_LINK
            .replace_all(&block.markdown, |capture: &regex::Captures<'_>| {
                base_url.join(&capture[1]).map_or_else(
                    |_| capture[0].to_owned(),
                    |url| format!("]({url})"),
                )
            })
            .into_owned();
    }
    ensure!(
        blocks
            .iter()
            .any(|block| block.kind == BlockKind::Paragraph),
        "HTML article contains no readable paragraphs"
    );
    if !blocks.iter().any(|block| {
        block.kind == BlockKind::Heading && block.text.eq_ignore_ascii_case("abstract")
    }) {
        blocks.insert(
            0,
            Block::content(
                BlockKind::Paragraph,
                metadata.abstract_text.clone(),
                metadata.abstract_text.clone(),
            ),
        );
        blocks.insert(0, Block::heading(2, "Abstract".to_owned()));
    }
    Ok(Document {
        origin,
        metadata,
        warnings: Vec::new(),
        blocks,
    })
}

fn has_class(element: ElementRef<'_>, class: &str) -> bool {
    element.value().classes().any(|value| value == class)
}

fn excluded(element: ElementRef<'_>) -> bool {
    matches!(
        element.value().name(),
        "script" | "style" | "nav" | "footer" | "button" | "noscript" | "form"
    ) || [
        "ltx_authors",
        "ltx_dates",
        "ltx_title_document",
        "ltx_page_header",
        "ltx_page_footer",
        "ltx_TOC",
        "ltx_navigation",
        "ltx_note_mark",
        "ltx_ERROR",
        "ltx_tag_equation",
    ]
    .iter()
    .any(|class| has_class(element, class))
}

fn collect(element: ElementRef<'_>, blocks: &mut Vec<Block>, in_references: bool) {
    if excluded(element) {
        return;
    }
    let name = element.value().name();
    let in_references = in_references || has_class(element, "ltx_bibliography");
    if name.len() == 2 && name.starts_with('h') {
        if let Ok(mut level) = name[1..].parse::<usize>() {
            let text = normalize(&inline(element));
            if has_class(element, "ltx_title_abstract") || text.eq_ignore_ascii_case("abstract") {
                level = 2;
            }
            if !text.is_empty() {
                blocks.push(Block::heading(level.clamp(2, 6), text));
            }
            return;
        }
    }
    if has_class(element, "ltx_bibitem") || (in_references && name == "li") {
        let markdown = normalize(&inline(element));
        blocks.push(Block::content(
            BlockKind::Reference,
            normalize(&plain_text(element)),
            markdown,
        ));
        return;
    }
    if has_class(element, "ltx_equation") || has_class(element, "ltx_equationgroup") {
        let equations: Vec<_> = element
            .select(&selector("math"))
            .map(math_source)
            .filter(|source| !source.is_empty())
            .collect();
        if !equations.is_empty() {
            let text = equations.join("\n");
            let markdown = equations
                .iter()
                .map(|equation| format!("$$\n{equation}\n$$"))
                .collect::<Vec<_>>()
                .join("\n\n");
            blocks.push(Block::content(BlockKind::Equation, text, markdown));
            return;
        }
    }
    if name == "figure" || has_class(element, "ltx_figure") || has_class(element, "ltx_table") {
        let table_selector = selector("table");
        let tables: Vec<_> = element.select(&table_selector).collect();
        let kind = if tables.is_empty() {
            BlockKind::Figure
        } else {
            BlockKind::Table
        };
        let caption = element
            .select(&selector("figcaption, .ltx_caption"))
            .map(|caption| normalize(&inline(caption)))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut content = Vec::new();
        if !caption.is_empty() {
            content.push(format!("### {caption}"));
        }
        for table in tables {
            content.push(render_table(table));
        }
        if kind == BlockKind::Figure {
            for image in element.select(&selector("img")) {
                if let Some(source) = image.value().attr("src").and_then(safe_link) {
                    let alternative = image.value().attr("alt").unwrap_or("Figure");
                    content.push(format!("![{}]({source})", escape_label(alternative)));
                }
            }
        }
        if !content.is_empty() {
            blocks.push(Block::content(
                kind,
                normalize(&plain_text(element)),
                content.join("\n\n"),
            ));
        }
        return;
    }
    if name == "table" {
        blocks.push(Block::content(
            BlockKind::Table,
            normalize(&plain_text(element)),
            render_table(element),
        ));
        return;
    }
    if name == "math" && element.value().attr("display") == Some("block") {
        let text = math_source(element);
        blocks.push(Block::content(
            BlockKind::Equation,
            text.clone(),
            format!("$$\n{text}\n$$"),
        ));
        return;
    }
    if name == "pre" {
        let text = element.text().collect::<String>();
        let fence = if text.contains("```") { "````" } else { "```" };
        blocks.push(Block::content(
            BlockKind::Code,
            text.clone(),
            format!("{fence}\n{text}\n{fence}"),
        ));
        return;
    }
    if matches!(name, "p" | "li" | "blockquote") {
        let content = normalize(&inline(element));
        if !content.is_empty() {
            let markdown = match name {
                "li" => format!("- {content}"),
                "blockquote" => format!("> {content}"),
                _ => content,
            };
            let references_heading = blocks
                .iter()
                .rev()
                .find(|block| block.kind == BlockKind::Heading)
                .is_some_and(|block| is_references(&block.text));
            let kind = if in_references || references_heading {
                BlockKind::Reference
            } else {
                BlockKind::Paragraph
            };
            blocks.push(Block::content(kind, normalize(&plain_text(element)), markdown));
        }
        return;
    }
    for child in element.children().filter_map(ElementRef::wrap) {
        collect(child, blocks, in_references);
    }
}

fn math_source(element: ElementRef<'_>) -> String {
    element
        .value()
        .attr("alttext")
        .map(str::to_owned)
        .or_else(|| {
            element
                .select(&selector("annotation[encoding='application/x-tex']"))
                .next()
                .map(|annotation| annotation.text().collect())
        })
        .unwrap_or_else(|| element_text(element))
}

fn escape_label(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

fn safe_link(value: &str) -> Option<String> {
    if value.starts_with("https://") || value.starts_with("http://") || value.starts_with('#') {
        Some(
            value
                .replace(' ', "%20")
                .replace('(', "%28")
                .replace(')', "%29"),
        )
    } else if value.contains(':') || value.chars().any(char::is_control) {
        None
    } else {
        // Relative image paths are resolved against the paper URL after parsing.
        Some(value.to_owned())
    }
}

fn plain_text(element: ElementRef<'_>) -> String {
    if excluded(element) {
        return String::new();
    }
    if element.value().name() == "math" {
        return math_source(element);
    }
    let mut output = String::new();
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            output.push_str(text);
        } else if let Some(child) = ElementRef::wrap(child) {
            output.push_str(&plain_text(child));
            if matches!(child.value().name(), "div" | "p" | "td" | "th" | "br") {
                output.push(' ');
            }
        }
    }
    output
}

fn inline(element: ElementRef<'_>) -> String {
    if excluded(element) {
        return String::new();
    }
    let name = element.value().name();
    if name == "math" {
        return format!("${}$", math_source(element));
    }
    if name == "br" {
        return " ".to_owned();
    }
    if name == "img" {
        return element.value().attr("alt").unwrap_or("").to_owned();
    }
    let mut content = String::new();
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            content.push_str(text);
        } else if let Some(child) = ElementRef::wrap(child) {
            content.push_str(&inline(child));
        }
    }
    match name {
        "strong" | "b" => format!("**{}**", content.trim()),
        "em" | "i" => format!("*{}*", content.trim()),
        "code" => format!("`{content}`"),
        "a" => {
            if let Some(link) = element.value().attr("href").and_then(safe_link) {
                if !link.starts_with('#') {
                    return format!("[{}]({link})", content.trim());
                }
            }
            content
        }
        _ => content,
    }
}

fn render_table(table: ElementRef<'_>) -> String {
    let mut grid: Vec<Vec<Option<String>>> = Vec::new();
    for (row_index, row) in table.select(&selector("tr")).take(2000).enumerate() {
        while grid.len() <= row_index {
            grid.push(Vec::new());
        }
        let mut column = 0;
        for cell in row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|cell| matches!(cell.value().name(), "td" | "th"))
        {
            while grid[row_index].get(column).is_some_and(Option::is_some) {
                column += 1;
            }
            let span = |attribute: &str| {
                cell.value()
                    .attr(attribute)
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 128)
            };
            let width = span("colspan");
            let height = span("rowspan");
            let content = normalize(&inline(cell)).replace('|', "\\|");
            for offset in 0..height {
                while grid.len() <= row_index + offset {
                    grid.push(Vec::new());
                }
                let target = &mut grid[row_index + offset];
                target.resize(target.len().max(column + width), None);
                for position in 0..width {
                    target[column + position] = Some(content.clone());
                }
            }
            column += width;
        }
    }
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return element_text(table);
    }
    let mut output = Vec::new();
    for (index, row) in grid.iter().enumerate() {
        let cells: Vec<_> = (0..width)
            .map(|column| row.get(column).and_then(Option::as_deref).unwrap_or(""))
            .collect();
        output.push(format!("| {} |", cells.join(" | ")));
        if index == 0 {
            output.push(format!("| {} |", vec!["---"; width].join(" | ")));
        }
    }
    output.join("\n")
}
