# CreepDir

Fast CLI that recursively catalogs files by extension. Parallel scan, skips
inaccessible folders, and outputs text, JSON, or CSV.

## Build

```bash
cargo build --release
```

## Usage

```
CreepDir [OPTIONS] [FOLDER] [OUTPUT]
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
