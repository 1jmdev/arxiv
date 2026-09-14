# arxiv

**Unofficial — not affiliated with or endorsed by arXiv.org.** A small Rust CLI for researchers using AI agents.

Fetch papers directly from arXiv web pages and turn them into clean Markdown with sections, equations, tables, references, and image links.

```sh
arxiv read 1706.03762
arxiv toc 1706.03762
arxiv section 1706.03762 "attention"
```

## Install

Requires [Rust](https://rustup.rs/).

```sh
cargo install --git https://github.com/1jmdev/arxiv --locked arxiv
```

From a local checkout:

```sh
cargo install --path . --locked
```

## Agent setup

Install the CLI first, then install the root [`SKILL.md`](SKILL.md) for your agent.

### OpenCode and Codex and most other harnesses

Both use `~/.agents/skills` for skills available across projects.

```sh
mkdir -p "$HOME/.agents/skills/arxiv-research"
curl -fsSL \
  https://raw.githubusercontent.com/1jmdev/arxiv/main/SKILL.md \
  -o "$HOME/.agents/skills/arxiv-research/SKILL.md"
```

### Claude Code

```sh
mkdir -p "$HOME/.claude/skills/arxiv-research"
curl -fsSL \
  https://raw.githubusercontent.com/1jmdev/arxiv/main/SKILL.md \
  -o "$HOME/.claude/skills/arxiv-research/SKILL.md"
```

Ask your agent to use `arxiv-research`. For a project-local installation, use `.agents/skills` or `.claude/skills` in that project instead.

## Commands

| Command | Output |
| --- | --- |
| `arxiv read <ID>` | Full paper as Markdown |
| `arxiv abstract <ID>` | Abstract only |
| `arxiv metadata <ID>` | Title, authors, dates, categories, and DOI |
| `arxiv versions <ID>` | Revision history |
| `arxiv toc <ID>` | Section hierarchy |
| `arxiv section <ID> <SECTION>` | Section by number or name |
| `arxiv search <QUERY>` | Matching papers and abstracts |
| `arxiv references <ID>` | Bibliography |
| `arxiv cite <ID>` | Plain citation |
| `arxiv bibtex <ID>` | BibTeX entry |
| `arxiv figures <ID>` | Figure captions and image URLs |
| `arxiv figure <ID> <FIGURE>` | Local image paths for a figure's panels |
| `arxiv source <ID>` | Cached source path |
| `arxiv download-source <ID> <PATH>` | Download original source |
| `arxiv pdf <ID> [PATH]` | Download PDF or print its cached path |
| `arxiv html <ID> [PATH]` | Download original HTML or print its cached path |

IDs can include a revision, such as `1706.03762v1`. Without one, the latest version is used. Legacy IDs and arXiv URLs are also accepted.

### Read and navigate

```sh
arxiv read 1706.03762v1
arxiv section 1706.03762 "3.2"
arxiv read 1706.03762 --from "3" --to "5"
arxiv read 1706.03762 --chunks
arxiv read 1706.03762 --chunk 2
arxiv read 1706.03762 --no-references --no-figures
arxiv read 1706.03762 --format json
arxiv metadata 1706.03762 --json
arxiv references 1706.03762 --ids
```

Sections include their subsections. Ranges are inclusive. Chunks are one-based and target 12,000 characters while keeping paragraphs, equations, and tables intact. Read formats: `markdown`, `text`, and `json`.

### Search

```sh
arxiv search "mixture of experts transformer"
arxiv search "reasoning" --category cs.AI --since 2025-01-01 --limit 20
arxiv search "representation learning" --author "Yann LeCun"
arxiv search "reasoning" --ids
arxiv search "reasoning" --json
```

Filters can be combined. Search returns 10 results by default, ordered by newest announcement.

### Images and source files

```sh
arxiv figures 2412.09282v2 --json
arxiv figure 2412.09282v2 1
arxiv figure 2412.09282v2 1 --original
arxiv source 1706.03762 --files
arxiv source 1706.03762 --file main.tex
arxiv pdf 1706.03762 paper.pdf
arxiv download-source 1706.03762 source.tar.gz
```

`figure` downloads every panel and converts SVG graphs to PNG for image viewers. Use `--original` to keep SVG files. Discover source filenames with `--files` before reading one. Downloads preserve the original payload and refuse to overwrite existing files.

## Cache and extraction

HTML is preferred; PDF text is the fallback. PDF extraction can lose equations, table layout, and reading order. Figure downloads require linked images in paper HTML.

Files are cached by paper version (`~/.cache/arxiv` on Linux). Unversioned metadata and search results refresh after one hour.

```sh
arxiv read 1706.03762 --refresh
arxiv read 1706.03762v7 --offline
arxiv read 1706.03762 --cache-dir ./research-cache
```

`ARXIV_CACHE_DIR` also sets the cache location. Output goes to stdout; diagnostics go to stderr. Run `arxiv <COMMAND> --help` for all options.

## Development

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
```

## License

[MIT](LICENSE). Papers retain their own licenses.
