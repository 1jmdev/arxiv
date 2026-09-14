# arxiv

**Unofficial. This project is not affiliated with or endorsed by arXiv.org.** It is a small Rust utility for researchers who use AI agents and need well-formatted research papers in their context.

Fetch arXiv web pages, isolate the research paper, and convert it to structured Markdown.

```sh
arxiv read 1706.03762
arxiv toc 1706.03762
arxiv section 1706.03762 "3.2"
```

## Install

Install a current stable [Rust toolchain](https://rustup.rs/), then:

```sh
cargo install --git https://github.com/1jmdev/arxiv --locked arxiv
```

Ensure Cargo's binary directory is on your `PATH` (`~/.cargo/bin` on Linux and macOS). Building requires the platform's C/C++ build tools for the TLS dependency. On Debian/Ubuntu, install `build-essential` and `cmake`; on macOS, install the Xcode command-line tools; on Windows, install the Visual Studio C++ build tools.

From a local checkout:

```sh
cargo install --path . --locked
```

## Read papers

```sh
arxiv read 1706.03762
arxiv read 1706.03762v1
arxiv abstract 1706.03762
arxiv metadata 1706.03762
arxiv metadata 1706.03762 --json
arxiv versions 1706.03762
```

Modern IDs, legacy IDs such as `hep-th/9901001`, and arXiv abstract/PDF/HTML URLs are accepted. An unversioned ID resolves to the latest revision listed on its abstract page. An explicit `vN` stays pinned to that revision.

The default output includes the title, authors, version, abstract, sections, equations, figure captions, tables, and references:

```markdown
# Attention Is All You Need

**Authors:** Ashish Vaswani, Noam Shazeer, ...

**arXiv:** 1706.03762v7

## Abstract

The dominant sequence transduction models ...

## 1 Introduction

...

### 3.2 Attention

...
```

The ellipses above abbreviate this example; the command prints the extracted content without summarizing it.

## Navigate and limit context

```sh
arxiv toc 1706.03762
arxiv section 1706.03762 "3.2"
arxiv section 1706.03762 "attention"
arxiv section 1706.03762 "results"
arxiv read 1706.03762 --section "conclusion"
arxiv read 1706.03762 --from "3" --to "5"
arxiv read 1706.03762 --chunks
arxiv read 1706.03762 --chunk 2
arxiv read 1706.03762 --no-references --no-figures
arxiv read 1706.03762 --format text
arxiv read 1706.03762 --format json
```

Section numbers and exact titles take precedence over case-insensitive partial matches. Ambiguous matches return an error listing candidates. Selecting a section includes its subsections. `--from` and `--to` form an inclusive range; either end can be omitted.

Chunks are one-based and target approximately 12,000 Unicode characters. Paragraphs, equations, and tables are kept intact, so an unusually large block can exceed that target. Chunking happens after section and inclusion filters. `--chunks --format json` returns a machine-readable chunk index.

References and figures are included by default; `--references` and `--figures` explicitly enable them. `--no-figures` removes figure images and captions while retaining tables. JSON reads contain `metadata`, `origin`, `warnings`, and ordered `blocks` with `kind`, `level`, `text`, and `markdown` fields. Text output removes Markdown presentation; equations remain textual mathematical expressions.

## Search

```sh
arxiv search "mixture of experts transformer"
arxiv search "mixture of experts transformer" --ids
arxiv search "mixture of experts transformer" --json
arxiv search "reasoning" --category cs.AI
arxiv search "reasoning" --author "Yann LeCun"
arxiv search "reasoning" --since 2025-01-01 --limit 20
```

Search uses arXiv's advanced **web search form**. Results include the ID, title, authors, and full abstract. Filters are combined with AND; categories include cross-listings. `--since` uses the original submission date. Results are ordered by newest announcement, with 10 returned by default. `--limit` accepts 1–1,000 and follows pagination as needed. Search syntax and author matching follow arXiv's web interface.

## References and citations

```sh
arxiv references 1706.03762
arxiv references 1706.03762 --ids
arxiv cite 1706.03762
arxiv bibtex 1706.03762
```

`references --ids` returns unique arXiv IDs explicitly present in bibliography text or links, in citation order. It does not guess missing identifiers or look them up elsewhere. `cite` produces a plain citation; `bibtex` generates an `@misc` entry from the abstract page, including the selected revision.

## Source, PDF, and HTML

```sh
arxiv source 1706.03762
arxiv source 1706.03762 --files
arxiv source 1706.03762 --file main.tex
arxiv download-source 1706.03762 source.tar.gz
arxiv pdf 1706.03762 paper.pdf
arxiv html 1706.03762 paper.html
```

Use `source --files` to discover the actual filenames before selecting one. `--file` prints UTF-8 text and rejects binary files. Source archives are inspected without extracting paths or executing TeX. Single-file submissions are listed as `source.tex`, `source.ps`, or `source.pdf` according to their contents.

`download-source` preserves the original bytes. Most submissions are gzip-compressed tar archives, but arXiv can supply a single compressed file or another source format; the destination extension does not change the payload.

With no destination, `source`, `pdf`, and `html` print an absolute local cache path. Binary data is never written to stdout. For example, on Linux:

```text
/home/you/.cache/arxiv/1706.03762v7/paper.pdf
```

`html` downloads the original paper HTML, while `read` produces cleaned Markdown. `html` reports an error when paper HTML is unavailable. Downloads to explicit destinations refuse to overwrite existing files.

## Agent skills

The portable skill lives in [`.skills/arxiv-research/SKILL.md`](.skills/arxiv-research/SKILL.md). Install the CLI first, then copy the skill into your agent's discovery directory. These commands work from any directory on a POSIX shell.

**Codex — available across projects:**

```sh
mkdir -p "$HOME/.agents/skills/arxiv-research"
curl --fail --silent --show-error --location \
  https://raw.githubusercontent.com/1jmdev/arxiv/main/.skills/arxiv-research/SKILL.md \
  --output "$HOME/.agents/skills/arxiv-research/SKILL.md"
```

Invoke with `$arxiv-research`, or ask Codex to research a paper. For a project-local installation, use `.agents/skills` instead of `$HOME/.agents/skills`.

**Claude Code — available across projects:**

```sh
mkdir -p "$HOME/.claude/skills/arxiv-research"
curl --fail --silent --show-error --location \
  https://raw.githubusercontent.com/1jmdev/arxiv/main/.skills/arxiv-research/SKILL.md \
  --output "$HOME/.claude/skills/arxiv-research/SKILL.md"
```

Invoke with `/arxiv-research`, or ask Claude to research a paper. For a project-local installation, use `.claude/skills` instead of `$HOME/.claude/skills`. Restart the agent if the skill does not appear.

From this checkout, you can also copy `.skills/arxiv-research` into either discovery directory. The `.skills` directory is the distribution source; agents discover its installed copy. The layout follows the official [Codex skill documentation](https://developers.openai.com/codex/skills/) and [Claude Code skill documentation](https://code.claude.com/docs/en/skills).

## Retrieval and cache

```text
arXiv ID → abstract web page → pinned version
                                  ↓
                        paper HTML, if available
                                  ↓ otherwise
                              PDF text
                                  ↓
                       structure → Markdown → stdout
```

The HTML path selects the paper article, removes site navigation and controls, and retains scientific content. MathML LaTeX annotations become `$...$` or `$$...$$`. Figures retain captions and image links; images are not analyzed. HTML tables become Markdown tables, with merged-cell values repeated across the cells they cover.

PDF fallback is best effort: section boundaries are heuristic, reading order can be imperfect, and mathematical notation, figures, and tables may be incomplete. Scanned PDFs require OCR, which is not included. Warnings go to stderr and appear in JSON reads. For exact notation, inspect the HTML, PDF, or source. Website layout changes and arXiv access restrictions can cause retrieval to fail; the CLI does not bypass access controls.

The cache uses the platform's cache directory (`~/.cache/arxiv` on Linux), with separate directories for each paper version. Abstract pages for unversioned IDs and search pages expire after one hour. Version-pinned resources remain cached until explicitly refreshed.

```sh
arxiv read 1706.03762 --refresh
arxiv read 1706.03762v7 --offline
arxiv read 1706.03762 --cache-dir ./research-cache
```

`ARXIV_CACHE_DIR` also overrides the location. `--offline` requires the relevant resources to be cached and makes no network requests. Requests are serialized and spaced by at least three seconds across processes sharing a cache. Transient HTTP failures receive bounded retries. Downloads are limited to 100 MiB and expanded source archives to 200 MiB.

Successful output goes to stdout; diagnostics go to stderr. Exit status is `0` on success, `1` for retrieval or processing errors, and `2` for invalid CLI arguments. Piping into tools such as `head` handles a closed pipe normally.

## Development

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
```

The project is a single Cargo package. Modules separate command handling, web retrieval, identifiers, metadata, HTML conversion, PDF extraction, document selection, search, citations, and source archives. Tests use local representative HTML fixtures and temporary caches; they do not require network access.

## License

MIT. Downloaded papers remain subject to their own licenses.
