use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::process::{Command, Output};

use arxiv::arguments::{Arguments, Command as ParsedCommand};
use clap::Parser;
use tempfile::TempDir;

fn prepare_cache() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path();
    fs::create_dir_all(cache.join("metadata")).unwrap();
    fs::create_dir_all(cache.join("2501.01234v2/figures")).unwrap();
    fs::write(
        cache.join("metadata/2501.01234.html"),
        include_str!("fixtures/abstract.html"),
    )
    .unwrap();
    fs::write(
        cache.join("2501.01234v2/paper.html"),
        include_str!("fixtures/article.html"),
    )
    .unwrap();
    fs::write(cache.join("2501.01234v2/paper.pdf"), b"%PDF-1.7\n").unwrap();
    let mut archive = tar::Builder::new(Vec::new());
    let contents = b"\\documentclass{article}\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "main.tex", &contents[..])
        .unwrap();
    fs::write(
        cache.join("2501.01234v2/source.archive"),
        archive.into_inner().unwrap(),
    )
    .unwrap();
    let image = resvg::tiny_skia::Pixmap::new(10, 10).unwrap();
    fs::write(
        cache.join("2501.01234v2/figures/figure-1-1.png"),
        image.encode_png().unwrap(),
    )
    .unwrap();
    directory
}

fn execute(cache: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arxiv"))
        .arg("--cache-dir")
        .arg(cache)
        .arg("--offline")
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn commands_work_from_cache_and_keep_stdout_machine_readable() {
    let directory = prepare_cache();
    let cases: &[(&[&str], &str)] = &[
        (&["abstract", "2501.01234"], "We evaluate"),
        (&["metadata", "2501.01234"], "Version: v2"),
        (&["versions", "2501.01234"], "v1  2025-01-01"),
        (&["toc", "2501.01234"], "  2.1 Attention"),
        (
            &["read", "2501.01234", "--section", "2.1"],
            "Scaled Attention",
        ),
        (&["section", "2501.01234", "results"], "3 Results"),
        (&["references", "2501.01234", "--ids"], "1607.06450"),
        (&["source", "2501.01234", "--files"], "main.tex"),
        (
            &["source", "2501.01234", "--file", "main.tex"],
            "\\documentclass",
        ),
        (&["source", "2501.01234"], "source.archive"),
        (&["pdf", "2501.01234"], "paper.pdf"),
        (&["html", "2501.01234"], "paper.html"),
        (&["cite", "2501.01234"], "arXiv:2501.01234v2"),
        (&["bibtex", "2501.01234"], "@misc{"),
        (&["figures", "2501.01234"], "Model architecture."),
        (&["figure", "2501.01234", "1"], "figure-1-1.png"),
    ];
    for (arguments, expected) in cases {
        let output = execute(directory.path(), arguments);
        assert!(output.status.success(), "{:?}: {:?}", arguments, output);
        assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
        assert!(output.stderr.is_empty());
    }
    // Resolving an unversioned paper also makes its pinned metadata available offline.
    let output = execute(directory.path(), &["metadata", "2501.01234v2", "--json"]);
    assert!(output.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(metadata["version"], "v2");
    let output = execute(
        directory.path(),
        &["read", "2501.01234", "--format", "json"],
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(document["blocks"].as_array().unwrap().len() > 10);
}

#[test]
fn downloads_preserve_payload_and_refuse_overwrites() {
    let directory = prepare_cache();
    let destination = directory.path().join("downloaded.archive");
    let arguments = [
        "download-source",
        "2501.01234",
        destination.to_str().unwrap(),
    ];
    assert!(execute(directory.path(), &arguments).status.success());
    assert_eq!(
        fs::read(&destination).unwrap(),
        fs::read(directory.path().join("2501.01234v2/source.archive")).unwrap()
    );
    assert!(!execute(directory.path(), &arguments).status.success());
}

#[test]
fn search_supports_offline_json_and_identifier_output() {
    let directory = prepare_cache();
    let arguments = Arguments::parse_from(["arxiv", "search", "research"]);
    let ParsedCommand::Search(arguments) = arguments.command else {
        panic!("expected search");
    };
    let url = arxiv::search::search_url(&arguments, 0).unwrap();
    let mut hash = DefaultHasher::new();
    url.as_str().hash(&mut hash);
    let path = directory.path().join("search");
    fs::create_dir_all(&path).unwrap();
    let html = include_str!("fixtures/search.html").replace(
        "class=\"pagination-next\"",
        "class=\"pagination-next is-invisible\"",
    );
    fs::write(path.join(format!("{:016x}.html", hash.finish())), html).unwrap();
    let output = execute(directory.path(), &["search", "research", "--json"]);
    assert!(output.status.success(), "{output:?}");
    let results: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(results[0]["id"], "2501.01234");
    let output = execute(directory.path(), &["search", "research", "--ids"]);
    assert_eq!(output.stdout, b"2501.01234\n");
}

#[test]
fn errors_are_nonzero_without_partial_stdout() {
    let directory = prepare_cache();
    for arguments in [
        vec!["read", "../secret"],
        vec!["read", "2501.01234", "--chunk", "0"],
        vec!["section", "2501.01234", "missing"],
        vec!["metadata", "2501.09999"],
    ] {
        let output = execute(directory.path(), &arguments);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let output = execute(directory.path(), &["search", "test", "--limit", "0"]);
    assert_eq!(output.status.code(), Some(2));
}
