use arxiv::arguments::{Arguments, Command};
use arxiv::search;
use clap::Parser;

#[test]
fn parses_complete_abstracts_and_excludes_doi_tags() {
    let (results, next) = search::parse(include_str!("fixtures/search.html")).unwrap();
    assert!(next);
    assert_eq!(
        results[0].abstract_text,
        "We evaluate structured documents."
    );
    assert_eq!(results[0].categories, ["cs.AI"]);
    assert_eq!(results[0].authors, ["Alice Example", "Bob Researcher"]);
}

#[test]
fn distinguishes_empty_results_from_access_challenges() {
    let (results, _) = search::parse("<p>Sorry, your query returned no results.</p>").unwrap();
    assert!(results.is_empty());
    assert!(search::parse("<p>Please verify you are human.</p>").is_err());
}

#[test]
fn encodes_filters_as_web_form_fields() {
    let arguments = Arguments::parse_from([
        "arxiv",
        "search",
        "reasoning & evaluation",
        "--category",
        "cs.AI",
        "--author",
        "Yann LeCun",
        "--since",
        "2025-01-01",
        "--limit",
        "20",
    ]);
    let Command::Search(arguments) = arguments.command else {
        panic!("expected search arguments");
    };
    let url = search::search_url(&arguments, 50).unwrap();
    assert_eq!(url.host_str(), Some("arxiv.org"));
    assert_eq!(url.path(), "/search/advanced");
    let parameters: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(parameters["terms-0-term"], "reasoning & evaluation");
    assert_eq!(parameters["terms-1-field"], "author");
    assert_eq!(parameters["terms-2-field"], "cross_list_category");
    assert_eq!(parameters["date-from_date"], "2025-01-01");
    assert_eq!(parameters["date-date_type"], "submitted_date_first");
    assert_eq!(parameters["start"], "50");
}
