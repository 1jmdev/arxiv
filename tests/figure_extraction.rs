use arxiv::{figures, html_document, metadata};

#[test]
fn preserves_svg_objects_and_panel_captions_in_document_order() {
    let metadata = metadata::parse(
        include_str!("fixtures/abstract.html"),
        &"2501.01234".parse().unwrap(),
    )
    .unwrap();
    let document =
        html_document::parse(include_str!("fixtures/figure_panels.html"), metadata).unwrap();
    let figures: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| block.figure.clone())
        .collect();
    assert_eq!(figures.len(), 1);
    assert_eq!(figures[0].assets.len(), 3);
    assert_eq!(figures[0].caption, "Comparison of quantization methods.");
    assert_eq!(figures[0].assets[0].label, "(a) uniform quantization");
    assert_eq!(figures[0].assets[1].label, "(b) vector quantization");
    assert_eq!(figures[0].assets[2].label, "(c) channel relaxation");
    assert_eq!(
        figures[0].assets[0].url,
        "https://arxiv.org/html/2501.01234v2/uniform.svg"
    );
    let markdown = document.markdown();
    assert!(markdown.contains("### Figure 1\n\nComparison"));
    assert_eq!(markdown.matches("![").count(), 3);
    assert!(!markdown.contains("- •"));
    assert_eq!(figures::select(&figures, "S1.F1").unwrap().number, 1);
    assert_eq!(figures::select(&figures, "Figure 1").unwrap().number, 1);
}

#[test]
fn renders_svg_graph_to_nonempty_png() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60">
        <rect x="10" y="10" width="80" height="40" fill="#ff0000"/>
    </svg>"##;
    let png = figures::render_svg(svg).unwrap();
    let image = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
    assert_eq!(image.width(), 300);
    assert_eq!(image.height(), 180);
    let center = image.pixel(150, 90).unwrap();
    assert_eq!(center.red(), 255);
    assert_eq!(center.green(), 0);
    assert_eq!(center.blue(), 0);
}
