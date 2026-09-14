use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

use crate::identifier::Identifier;
use crate::web_client::WebClient;

static VERSION_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(v\d+)\]\s*[A-Za-z]+,\s*(\d{1,2}\s+[A-Za-z]+\s+\d{4})")
        .expect("valid version expression")
});
static CATEGORY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\(([a-z][a-z-]*(?:\.[A-Z]{2})?)\)").expect("valid category expression")
});

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Version {
    pub version: String,
    pub submitted: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Metadata {
    pub id: String,
    pub version: String,
    pub title: String,
    pub authors: Vec<String>,
    pub submitted: String,
    pub updated: String,
    pub categories: Vec<String>,
    pub doi: Option<String>,
    pub abstract_text: String,
    pub url: String,
    pub versions: Vec<Version>,
}

impl Metadata {
    pub fn identifier(&self) -> Identifier {
        Identifier {
            base: self.id.clone(),
            version: Some(self.version.clone()),
        }
    }

    pub fn display(&self) -> String {
        format!(
            "ID: {}\nVersion: {}\nTitle: {}\nAuthors: {}\nSubmitted: {}\nUpdated: {}\nCategories: {}\nDOI: {}\n",
            self.id,
            self.version,
            self.title,
            self.authors.join("; "),
            self.submitted,
            self.updated,
            self.categories.join(", "),
            self.doi.as_deref().unwrap_or("")
        )
    }
}

pub fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("valid internal CSS selector")
}

pub fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn element_text(element: ElementRef<'_>) -> String {
    normalize(&element.text().collect::<String>())
}

pub fn load(client: &WebClient, identifier: &Identifier) -> Result<Metadata> {
    let relative = Path::new("metadata").join(format!("{}.html", identifier.cache_key()));
    let age = identifier.version.is_none().then_some(Duration::from_secs(3600));
    let path = client.resource(
        &format!("https://arxiv.org/abs/{identifier}"),
        &relative,
        age,
    )?;
    parse(&fs::read_to_string(path)?, identifier)
}

pub fn parse(html: &str, requested: &Identifier) -> Result<Metadata> {
    let document = Html::parse_document(html);
    let meta = |name: &str| -> Option<String> {
        document
            .select(&selector("meta[name][content]"))
            .find(|element| element.value().attr("name") == Some(name))
            .and_then(|element| element.value().attr("content"))
            .map(normalize)
    };
    let page_id: Identifier = meta("citation_arxiv_id")
        .context("page does not contain arXiv citation metadata")?
        .parse()?;
    ensure!(page_id.base == requested.base, "arXiv returned a different paper");
    let title = meta("citation_title").context("paper title is missing")?;
    let history = document
        .select(&selector(".submission-history"))
        .next()
        .map(element_text)
        .context("submission history is missing")?;
    let mut versions = Vec::new();
    for captures in VERSION_DATE.captures_iter(&history) {
        let date = NaiveDate::parse_from_str(&captures[2], "%e %b %Y")?;
        versions.push(Version {
            version: captures[1].to_owned(),
            submitted: date.to_string(),
        });
    }
    ensure!(!versions.is_empty(), "could not parse submission history");
    let version = requested.version.clone().unwrap_or_else(|| {
        versions
            .iter()
            .max_by_key(|version| version.version[1..].parse::<u32>().unwrap_or(0))
            .expect("nonempty versions")
            .version
            .clone()
    });
    let updated = versions
        .iter()
        .find(|entry| entry.version == version)
        .context("requested version does not exist")?
        .submitted
        .clone();
    // Verify that a redirect has not silently substituted the latest revision.
    if let Some(displayed) = document.select(&selector(".arxividv a")).next() {
        let displayed: Identifier = element_text(displayed).parse()?;
        ensure!(
            displayed.version.as_deref() == Some(&version),
            "arXiv returned a different version than requested"
        );
    }
    let mut authors: Vec<String> = document
        .select(&selector(".authors a"))
        .map(element_text)
        .collect();
    if authors.is_empty() {
        authors = document
            .select(&selector("meta[name='citation_author']"))
            .filter_map(|element| element.value().attr("content"))
            .map(normalize)
            .collect();
    }
    let subjects = document
        .select(&selector(".subjects"))
        .next()
        .map(element_text)
        .unwrap_or_default();
    let categories = CATEGORY
        .captures_iter(&subjects)
        .map(|capture| capture[1].to_owned())
        .collect();
    let doi = meta("citation_doi").or_else(|| {
        document
            .select(&selector("a[href*='doi.org/']"))
            .filter_map(|element| element.value().attr("href"))
            .find_map(|href| href.split_once("doi.org/").map(|(_, doi)| doi.to_owned()))
    });
    let submitted = versions
        .iter()
        .find(|entry| entry.version == "v1")
        .context("initial submission date is missing")?
        .submitted
        .clone();
    Ok(Metadata {
        id: requested.base.clone(),
        url: format!("https://arxiv.org/abs/{}{version}", requested.base),
        version,
        title,
        authors,
        submitted,
        updated,
        categories,
        doi,
        abstract_text: meta("citation_abstract").context("abstract is missing")?,
        versions,
    })
}
