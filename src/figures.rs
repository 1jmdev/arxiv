use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use scraper::ElementRef;
use serde::{Deserialize, Serialize};

use crate::html_document::inline;
use crate::metadata::{Metadata, element_text, normalize, selector};
use crate::web_client::WebClient;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FigureAsset {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Figure {
    pub number: usize,
    pub id: String,
    pub label: String,
    pub caption: String,
    pub assets: Vec<FigureAsset>,
}

pub fn extract(element: ElementRef<'_>, number: usize) -> Figure {
    let caption_element = own_caption(element);
    let full_caption = caption_element
        .map(|caption| normalize(&inline(caption)))
        .unwrap_or_default();
    let label = caption_element
        .and_then(|caption| caption.select(&selector(".ltx_tag_figure")).next())
        .map(element_text)
        .map(|label| label.trim_end_matches([':', ' ']).to_owned())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| {
            full_caption
                .split_once(':')
                .filter(|(label, _)| label.starts_with("Figure ") || label.starts_with("Fig."))
                .map_or_else(|| format!("Figure {number}"), |(label, _)| label.to_owned())
        });
    let caption = full_caption
        .strip_prefix(&label)
        .unwrap_or(&full_caption)
        .trim_start_matches([':', ' '])
        .to_owned();
    let mut assets = Vec::new();
    for image in element.select(&selector("img[src], object[data], embed[src]")) {
        let source = image
            .value()
            .attr("src")
            .or_else(|| image.value().attr("data"));
        let Some(source) = source else {
            continue;
        };
        let source = source.trim();
        if source.is_empty() || source.to_lowercase().starts_with("data:") {
            continue;
        }
        let panel = image
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|ancestor| ancestor.value().name() == "figure");
        let panel_caption = panel
            .filter(|panel| panel.id() != element.id())
            .and_then(own_caption)
            .map(|caption| normalize(&inline(caption)));
        let asset_label = panel_caption.unwrap_or_else(|| {
            image
                .value()
                .attr("alt")
                .filter(|value| !value.is_empty() && *value != "Refer to caption")
                .map_or_else(|| label.clone(), normalize)
        });
        if !assets.iter().any(|asset: &FigureAsset| asset.url == source) {
            assets.push(FigureAsset {
                label: asset_label,
                url: source.to_owned(),
            });
        }
    }
    Figure {
        number,
        id: element.value().attr("id").unwrap_or("").to_owned(),
        label,
        caption,
        assets,
    }
}

fn own_caption(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    element
        .select(&selector("figcaption, .ltx_caption"))
        .find(|caption| {
            caption
                .ancestors()
                .filter_map(ElementRef::wrap)
                .find(|ancestor| {
                    ancestor.value().name() == "figure"
                        || ancestor
                            .value()
                            .classes()
                            .any(|class| class == "ltx_figure")
                })
                .is_some_and(|ancestor| ancestor.id() == element.id())
        })
}

impl Figure {
    pub fn resolve_urls(&mut self, base_url: &reqwest::Url) {
        self.assets.retain_mut(|asset| {
            let Ok(url) = base_url.join(&asset.url) else {
                return false;
            };
            if !matches!(url.scheme(), "http" | "https") {
                return false;
            }
            asset.url = url.to_string();
            true
        });
    }

    pub fn markdown(&self) -> String {
        let mut parts = vec![format!("### {}", self.label)];
        if !self.caption.is_empty() {
            parts.push(self.caption.clone());
        }
        for asset in &self.assets {
            let label = asset.label.replace('[', "\\[").replace(']', "\\]");
            let url = asset.url.replace('(', "%28").replace(')', "%29");
            parts.push(format!("![{label}]({url})"));
        }
        if self.assets.is_empty() {
            parts.push("Image asset unavailable in this HTML figure.".to_owned());
        }
        parts.join("\n\n")
    }
}

pub fn select<'a>(figures: &'a [Figure], query: &str) -> Result<&'a Figure> {
    let query = query.trim();
    let matches: Vec<_> = figures
        .iter()
        .filter(|figure| {
            figure.number.to_string() == query
                || figure.id.eq_ignore_ascii_case(query)
                || figure.label.eq_ignore_ascii_case(query)
        })
        .collect();
    ensure!(
        matches.len() == 1,
        "figure {query:?} is missing or ambiguous; use `arxiv figures <ID>`"
    );
    Ok(matches[0])
}

pub fn download(
    client: &WebClient,
    metadata: &Metadata,
    figure: &Figure,
    original: bool,
) -> Result<Vec<PathBuf>> {
    ensure!(
        !figure.assets.is_empty(),
        "this figure has no downloadable assets"
    );
    let mut paths = Vec::new();
    for (index, asset) in figure.assets.iter().enumerate() {
        let url = reqwest::Url::parse(&asset.url)?;
        let extension = Path::new(url.path())
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("image")
            .to_lowercase();
        ensure!(
            ["svg", "png", "jpg", "jpeg", "gif", "webp"].contains(&extension.as_str()),
            "unsupported figure format: {extension}; original URL: {}",
            asset.url
        );
        let filename = format!("figure-{}-{}.{}", figure.number, index + 1, extension);
        let relative = Path::new(&metadata.identifier().cache_key())
            .join("figures")
            .join(filename);
        let path = client.resource(&asset.url, &relative, None, |bytes| {
            validate_image(bytes, &extension)
        })?;
        if extension == "svg" && !original {
            let rendered = path.with_extension("png");
            if !rendered.is_file() || client.refresh {
                let png = render_svg(&fs::read(&path)?)?;
                WebClient::write_atomic(&rendered, &png)?;
            }
            paths.push(fs::canonicalize(rendered)?);
        } else {
            paths.push(fs::canonicalize(path)?);
        }
    }
    Ok(paths)
}

fn validate_image(bytes: &[u8], extension: &str) -> Result<()> {
    let valid = match extension {
        "svg" => {
            let text = std::str::from_utf8(bytes)?;
            text.contains("<svg") && !text.to_lowercase().contains("<html")
        }
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => bytes.starts_with(b"GIF8"),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        _ => false,
    };
    ensure!(valid, "arXiv did not return a valid {extension} image");
    Ok(())
}

pub fn render_svg(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    // Do not let an SVG read local files through external image references.
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = resvg::usvg::Tree::from_data(bytes, &options).context("parsing SVG figure")?;
    let size = tree.size();
    let scale = (1600.0 / size.width().max(size.height())).clamp(0.1, 3.0);
    let width = (size.width() * scale).ceil() as u32;
    let height = (size.height() * scale).ceil() as u32;
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 16_000_000,
        "figure dimensions exceed the rendering limit"
    );
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).context("allocating figure image")?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap.encode_png()?)
}
