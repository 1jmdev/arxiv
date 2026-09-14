use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use scraper::Html;
use serde::Serialize;

use crate::arguments::SearchArguments;
use crate::identifier::Identifier;
use crate::metadata::{element_text, normalize, selector};
use crate::web_client::WebClient;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub abstract_text: String,
    pub categories: Vec<String>,
    pub url: String,
}

pub fn search_url(arguments: &SearchArguments, start: usize) -> Result<reqwest::Url> {
    ensure!(
        !arguments.query.trim().is_empty(),
        "search query must not be empty"
    );
    let mut url = reqwest::Url::parse("https://arxiv.org/search/advanced")?;
    {
        let mut parameters = url.query_pairs_mut();
        parameters
            .append_pair("advanced", "1")
            .append_pair("terms-0-operator", "AND")
            .append_pair("terms-0-term", &arguments.query)
            .append_pair("terms-0-field", "all")
            .append_pair("classification-include_cross_list", "include")
            .append_pair("abstracts", "show")
            .append_pair("size", "50")
            .append_pair("order", "-announced_date_first")
            .append_pair("start", &start.to_string());
        let mut index = 1;
        for (field, value) in [
            ("author", arguments.author.as_ref()),
            ("cross_list_category", arguments.category.as_ref()),
        ] {
            if let Some(value) = value {
                ensure!(!value.trim().is_empty(), "{field} filter must not be empty");
                parameters
                    .append_pair(&format!("terms-{index}-operator"), "AND")
                    .append_pair(&format!("terms-{index}-term"), value)
                    .append_pair(&format!("terms-{index}-field"), field);
                index += 1;
            }
        }
        if let Some(since) = arguments.since {
            parameters
                .append_pair("date-filter_by", "date_range")
                .append_pair("date-from_date", &since.to_string())
                .append_pair("date-to_date", "")
                .append_pair("date-date_type", "submitted_date_first");
        } else {
            parameters.append_pair("date-filter_by", "all_dates");
        }
    }
    Ok(url)
}

pub fn execute(client: &WebClient, arguments: &SearchArguments) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();
    let mut identifiers = HashSet::new();
    let limit = usize::from(arguments.limit);
    let mut start = 0;
    loop {
        let url = search_url(arguments, start)?;
        let mut hash = DefaultHasher::new();
        url.as_str().hash(&mut hash);
        let relative = Path::new("search").join(format!("{:016x}.html", hash.finish()));
        let path = client.resource(
            url.as_str(),
            &relative,
            Some(Duration::from_secs(3600)),
            |bytes| parse(std::str::from_utf8(bytes)?).map(|_| ()),
        )?;
        let html = std::fs::read_to_string(path)?;
        let (page, next) = parse(&html)?;
        let count = page.len();
        for result in page {
            if identifiers.insert(result.id.clone()) {
                results.push(result);
                if results.len() == limit {
                    return Ok(results);
                }
            }
        }
        if !next || count == 0 {
            break;
        }
        ensure!(start < 10_000, "search pagination exceeded 10,000 results");
        start += 50;
    }
    Ok(results)
}

pub fn parse(html: &str) -> Result<(Vec<SearchResult>, bool)> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();
    for result in document.select(&selector("li.arxiv-result")) {
        let link = result
            .select(&selector(".list-title a"))
            .next()
            .context("search result is missing its paper identifier")?;
        let identifier: Identifier = link
            .value()
            .attr("href")
            .context("search result link is missing")?
            .parse()?;
        let title = result
            .select(&selector("p.title"))
            .next()
            .map(element_text)
            .context("search result title is missing")?;
        let authors = result
            .select(&selector(".authors a"))
            .map(element_text)
            .collect();
        let abstract_element = result
            .select(&selector(".abstract-full"))
            .next()
            .context("search result abstract is missing")?;
        let mut abstract_text = String::new();
        for child in abstract_element.children() {
            if let Some(text) = child.value().as_text() {
                abstract_text.push_str(text);
            } else if let Some(element) = scraper::ElementRef::wrap(child)
                && (element.value().name() != "a" || element.value().attr("onclick").is_none())
            {
                abstract_text.push_str(&element.text().collect::<String>());
            }
        }
        results.push(SearchResult {
            id: identifier.base.clone(),
            title,
            authors,
            abstract_text: normalize(&abstract_text),
            categories: result
                .select(&selector(".tags .tag[data-tooltip]"))
                .map(element_text)
                .collect(),
            url: format!("https://arxiv.org/abs/{}", identifier.base),
        });
    }
    if results.is_empty() {
        let body = document
            .root_element()
            .text()
            .collect::<String>()
            .to_lowercase();
        ensure!(
            body.contains("no results") || body.contains("sorry, your query"),
            "arXiv returned an unrecognized search page; it may be a challenge or a changed layout"
        );
    }
    let next = document
        .select(&selector("a.pagination-next[href]"))
        .any(|element| {
            !element
                .value()
                .classes()
                .any(|class| class == "is-invisible")
        });
    Ok((results, next))
}
