use arxiv::arguments::ReadArguments;
use arxiv::document::BlockKind;
use arxiv::{html_document, metadata, pdf_document};

const ABSTRACT: &str = include_str!("fixtures/abstract.html");
const ARTICLE: &str = include_str!("fixtures/article.html");

fn document() -> arxiv::document::Document {
    let metadata = metadata::parse(ABSTRACT, &"2501.01234".parse().unwrap()).unwrap();
    html_document::parse(ARTICLE, metadata).unwrap()
}

#[test]
fn retains_science_and_removes_site_content() {
    let document = document();
    let markdown = document.markdown();
    assert!(markdown.starts_with("# Structured Research & Evaluation"));
    assert!(markdown.contains("$$\nA(Q,K,V)=\\mathrm{softmax}(QK^T)V\n$$"));
    assert!(markdown.contains("| Method A | 28.4 |"));
    assert!(markdown.contains("| Method A | 29.1 |"));
    assert!(markdown.contains("### Figure 1: Model architecture."));
    assert!(markdown.contains("https://arxiv.org/html/2501.01234v2/figures/model.png"));
    for unwanted in ["Site navigation", "Website footer", "Duplicate authors", "malicious_script"] {
        assert!(!markdown.contains(unwanted));
    }
    assert_eq!(markdown.matches("# Structured Research").count(), 1);
    assert!(!document.text().contains("xx_i"));
}

#[test]
fn selects_number_and_title_with_descendants() {
    for section in ["2.1", "attention", "Attention"] {
        let selected = document()
            .select(&ReadArguments {
                section: Some(section.to_owned()),
                ..ReadArguments::default()
            })
            .unwrap();
        assert_eq!(selected.blocks[0].text, "2.1 Attention");
        assert!(selected.markdown().contains("Scaled Attention"));
        assert!(!selected.markdown().contains("2.2 Cross Attention"));
    }
}

#[test]
fn rejects_ambiguous_missing_and_reversed_sections() {
    let document = document();
    for selector in ["att", "nonexistent", ""] {
        assert!(document.select(&ReadArguments {
            section: Some(selector.to_owned()),
            ..ReadArguments::default()
        }).is_err());
    }
    assert!(document.select(&ReadArguments {
        from: Some("3".to_owned()),
        to: Some("2".to_owned()),
        ..ReadArguments::default()
    }).is_err());
}

#[test]
fn filters_figures_and_references_without_removing_tables_or_appendix() {
    let selected = document()
        .select(&ReadArguments {
            no_figures: true,
            no_references: true,
            ..ReadArguments::default()
        })
        .unwrap();
    assert!(!selected.markdown().contains("Figure 1"));
    assert!(!selected.markdown().contains("1607.06450"));
    assert!(selected.markdown().contains("Table 1"));
    assert!(selected.markdown().contains("Supplementary derivation"));
}

#[test]
fn citation_identifiers_are_explicit_deduplicated_and_ordered() {
    assert_eq!(document().reference_ids(), ["1607.06450", "hep-th/9901001v2"]);
}

#[test]
fn chunks_preserve_every_block_including_large_equations() {
    let mut document = document();
    document.blocks[3].markdown = "α".repeat(13_000);
    let chunks = document.chunks();
    assert!(chunks.len() > 1);
    let restored: Vec<_> = chunks.iter().flatten().map(|block| &block.markdown).collect();
    let original: Vec<_> = document.blocks.iter().map(|block| &block.markdown).collect();
    assert_eq!(restored, original);
}

#[test]
fn metadata_is_versioned_and_rejects_substituted_pages() {
    let metadata = metadata::parse(ABSTRACT, &"2501.01234".parse().unwrap()).unwrap();
    assert_eq!(metadata.version, "v2");
    assert_eq!(metadata.submitted, "2025-01-01");
    assert_eq!(metadata.updated, "2025-01-03");
    assert_eq!(metadata.categories, ["cs.AI", "cs.LG"]);
    assert!(metadata::parse(ABSTRACT, &"2501.01234v1".parse().unwrap()).is_err());
    assert!(metadata::parse(ABSTRACT, &"2501.09999".parse().unwrap()).is_err());
    let earlier = ABSTRACT.replace("arXiv:2501.01234v2", "arXiv:2501.01234v1");
    let earlier = metadata::parse(&earlier, &"2501.01234v1".parse().unwrap()).unwrap();
    assert_eq!(earlier.updated, "2025-01-01");
}

#[test]
fn refuses_non_paper_html() {
    let metadata = document().metadata;
    assert!(html_document::parse("<h1>Access denied</h1>", metadata).is_err());
}

#[test]
fn pdf_text_has_heuristic_sections_and_references() {
    let metadata = document().metadata;
    let blocks = pdf_document::parse_text(
        "Paper title\nAuthors\n\n1 Introduction\nFirst paragraph.\n\n2 Method\nMethod description.\n\nReferences\n[1] Earlier work.\nContinuation.\n\nA Appendix\nAdditional text.",
        &metadata,
    );
    assert!(blocks.iter().any(|block| block.text == "2 Method"));
    assert!(blocks.iter().any(|block| {
        block.kind == BlockKind::Reference && block.text.contains("Continuation.")
    }));
    assert_eq!(blocks.last().unwrap().kind, BlockKind::Paragraph);
}
