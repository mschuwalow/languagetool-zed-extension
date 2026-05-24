# Zed LanguageTool

LanguageTool integration for Zed. The Zed extension launches a Rust language server, which sends document text to a local or cloud LanguageTool HTTP API and publishes diagnostics and quick fixes through LSP.

## Local Development

Run a LanguageTool server on `localhost:8081`, then build the language server from the sibling `languagetool-lsp` repository:

```sh
cargo build --manifest-path ../languagetool-lsp/Cargo.toml -p languagetool-lsp
cp ../languagetool-lsp/target/debug/languagetool-lsp .
```

Install this repository as a Zed dev extension. The extension prefers a `languagetool-lsp` on `PATH`, then a downloaded release binary.

Example Zed settings:

```json
{
  "lsp": {
    "languagetool": {
      "initialization_options": {
        "backend": { "type": "local", "url": "http://localhost:8081" },
        "language": "en-US",
        "checkOnSave": true,
        "checkWhileTyping": false
      }
    }
  }
}
```

## LanguageTool Backends

Local backend, default:

```json
{
  "backend": { "type": "local", "url": "http://localhost:8081" }
}
```

Cloud backend:

```json
{
  "backend": { "type": "cloud" },
  "language": "auto",
  "preferredVariants": ["en-US"]
}
```

## Commands

```sh
cargo run --manifest-path ../languagetool-lsp/Cargo.toml -p languagetool-lsp -- --root . health
cargo run --manifest-path ../languagetool-lsp/Cargo.toml -p languagetool-lsp -- --root . check test-fixtures/plaintext/basic.txt
```

## Project Config

Workspace quick fixes can create or update `.zed/languagetool.json` by default.

```json
{
  "ignored_words": ["zed"],
  "disabled_rules": ["WHITESPACE_RULE"],
  "disabled_categories": ["TYPOGRAPHY"]
}
```

The LSP merges this file with Zed `initialization_options`. Values from both sources are kept for ignored words and disabled rules/categories.

The path can be changed with `projectConfigPath` for non-Zed workflows:

```json
{
  "projectConfigPath": ".idea/languagetool.json"
}
```

Available command-backed quick fixes:

- Ignore a single matched word in the workspace.
- Disable a LanguageTool rule in the workspace.
- Disable a LanguageTool category in the workspace.

After one of these commands updates `.zed/languagetool.json`, the server rechecks open documents.

## Current Scope

- Plain text, Markdown, MDX, HTML, LaTeX, Typst, and a conservative set of common code languages are registered in Zed.
- Markdown and HTML are sent with LanguageTool `data.annotation`, marking code fences, inline code, links, tags, scripts, and styles as markup.
- Code languages currently send comments as text annotations and code as markup, with Python triple-quoted strings treated as documentation.
- Quick fixes are generated from LanguageTool replacements.
- Ignore/disable actions persist to the configured project config path.
- On-change checking is debounced when `checkWhileTyping` is enabled.
