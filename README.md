# CreepDir

[![crates.io](https://img.shields.io/crates/v/creepdir.svg)](https://crates.io/crates/creepdir)
[![downloads](https://img.shields.io/crates/d/creepdir.svg)](https://crates.io/crates/creepdir)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
[![msrv](https://img.shields.io/badge/rustc-1.88+-lightgray.svg)](https://www.rust-lang.org)

Fast CLI that recursively catalogs files by extension. Parallel scan, skips
inaccessible folders, and outputs text, JSON, or CSV.

## Install

```bash
cargo install creepdir
```

## Build

```bash
cargo build --release
```

## Usage

```
creepdir [OPTIONS] [FOLDER] [OUTPUT]
```

Omit `OUTPUT` to write `<folder>.<ext>` inside the scanned folder.

| Flag | Description |
|------|-------------|
| `-s, --select` | Pick folder/output via dialogs |
| `-q, --quiet` | Hide "skipping" warnings |
| `-j, --threads <N>` | Worker threads (default: CPU cores) |
| `--follow-symlinks` | Follow symlinks/junctions |
| `--max-depth <N>` | Max depth (`0` = root only) |
| `--ext rs,txt` | Only these extensions |
| `-e, --exclude <GLOB>` | Exclude by glob (repeatable) |
| `--sizes` | Include file sizes |
| `--json` / `--csv` | Output format (default: text) |
