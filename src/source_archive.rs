use std::io::{Cursor, Read};
use std::path::{Component, Path};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;

const MAX_EXPANDED_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug)]
pub struct SourceFile {
    pub name: String,
    pub contents: Vec<u8>,
}

pub fn read(path: &Path) -> Result<Vec<SourceFile>> {
    parse(&std::fs::read(path)?)
}

pub fn parse(bytes: &[u8]) -> Result<Vec<SourceFile>> {
    let mut expanded = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(bytes)
            .take(MAX_EXPANDED_BYTES + 1)
            .read_to_end(&mut expanded)
            .context("decompressing source archive")?;
    } else {
        expanded.extend_from_slice(bytes);
    }
    ensure!(
        expanded.len() as u64 <= MAX_EXPANDED_BYTES,
        "expanded source exceeds the 200 MiB limit"
    );
    ensure!(!expanded.is_empty(), "source archive is empty");
    if !is_tar(&expanded) {
        let name = if expanded.starts_with(b"%PDF-") {
            "source.pdf"
        } else if expanded.starts_with(b"%!") {
            "source.ps"
        } else {
            ensure!(
                std::str::from_utf8(&expanded).is_ok(),
                "unrecognized binary source format; use download-source to inspect the original payload"
            );
            "source.tex"
        };
        return Ok(vec![SourceFile {
            name: name.to_owned(),
            contents: expanded,
        }]);
    }
    let mut archive = tar::Archive::new(Cursor::new(expanded));
    let mut files = Vec::new();
    let mut total = 0;
    for entry in archive.entries()? {
        let entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        ensure!(files.len() < 10_000, "archive contains too many files");
        let path = entry.path()?;
        let name = safe_name(&path)?;
        total += entry.size();
        ensure!(
            total <= MAX_EXPANDED_BYTES,
            "source files exceed the 200 MiB limit"
        );
        let mut contents = Vec::new();
        entry
            .take(MAX_EXPANDED_BYTES + 1)
            .read_to_end(&mut contents)?;
        files.push(SourceFile { name, contents });
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    ensure!(!files.is_empty(), "archive contains no regular files");
    Ok(files)
}

fn safe_name(path: &Path) -> Result<String> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => {
                let value = value.to_str().context("archive filename is not UTF-8")?;
                ensure!(
                    !value.chars().any(char::is_control) && !value.contains('\\'),
                    "unsafe archive filename"
                );
                components.push(value);
            }
            _ => bail!("unsafe path in source archive: {}", path.display()),
        }
    }
    ensure!(!components.is_empty(), "empty archive filename");
    Ok(components.join("/"))
}

fn is_tar(bytes: &[u8]) -> bool {
    if bytes.len() < 512 {
        return false;
    }
    let checksum = String::from_utf8_lossy(&bytes[148..156]);
    let Ok(expected) = u64::from_str_radix(checksum.trim_matches(['\0', ' ']), 8) else {
        return false;
    };
    let actual: u64 = bytes[..512]
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                32
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    expected == actual
}

pub fn file_text(files: &[SourceFile], requested: &str) -> Result<String> {
    let requested = safe_name(Path::new(requested))?;
    let matches: Vec<_> = files.iter().filter(|file| file.name == requested).collect();
    ensure!(
        matches.len() <= 1,
        "archive contains duplicate entries for {requested}"
    );
    let file = matches.first().with_context(|| {
        format!("source file {requested:?} not found; use `arxiv source <ID> --files`")
    })?;
    ensure!(
        !file.contents.contains(&0) && !file.contents.starts_with(b"%PDF-"),
        "requested file is binary; use download-source"
    );
    String::from_utf8(file.contents.clone()).context("source file is not UTF-8 text")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_compressed_single_file() {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(b"\\documentclass{article}").unwrap();
        let files = parse(&encoder.finish().unwrap()).unwrap();
        assert_eq!(files[0].name, "source.tex");
        assert_eq!(
            file_text(&files, "source.tex").unwrap(),
            "\\documentclass{article}"
        );
    }

    #[test]
    fn rejects_unsafe_names() {
        for path in ["../main.tex", "/etc/passwd", "a\nb", "a\\b"] {
            assert!(safe_name(Path::new(path)).is_err());
        }
    }
}
