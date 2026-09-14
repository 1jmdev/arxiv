use crate::metadata::Metadata;

pub fn citation(metadata: &Metadata) -> String {
    format!(
        "{}. {}. arXiv:{}{} ({}). {}\n",
        metadata.authors.join("; "),
        metadata.title,
        metadata.id,
        metadata.version,
        &metadata.submitted[..4],
        metadata.url
    )
}

fn escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\textbackslash{}"),
            '{' | '}' | '%' | '&' | '#' | '_' => {
                output.push('\\');
                output.push(character);
            }
            '~' => output.push_str("\\textasciitilde{}"),
            '^' => output.push_str("\\textasciicircum{}"),
            _ => output.push(character),
        }
    }
    output
}

pub fn bibtex(metadata: &Metadata) -> String {
    let key = format!(
        "arxiv{}{}",
        metadata.id.replace(['/', '.'], ""),
        metadata.version
    );
    let authors = metadata
        .authors
        .iter()
        .map(|author| escape(author))
        .collect::<Vec<_>>()
        .join(" and ");
    let mut fields = vec![
        ("title", format!("{{{}}}", escape(&metadata.title))),
        ("author", authors),
        ("year", metadata.submitted[..4].to_owned()),
        ("eprint", format!("{}{}", metadata.id, metadata.version)),
        ("archivePrefix", "arXiv".to_owned()),
        ("url", metadata.url.clone()),
    ];
    if let Some(category) = metadata.categories.first() {
        fields.push(("primaryClass", category.clone()));
    }
    if let Some(doi) = &metadata.doi {
        fields.push(("doi", escape(doi)));
    }
    let fields = fields
        .iter()
        .map(|(name, value)| format!("  {name} = {{{value}}}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("@misc{{{key},\n{fields}\n}}\n")
}
