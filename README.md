# List PRs

A command-line tool written in Rust that lists all your GitHub Pull Requests across repositories in an interactive interface.

## Prerequisites

- Rust and Cargo (install from [rustup.rs](https://rustup.rs))
- `fzf` command-line fuzzy finder
- `bat` for syntax highlighting in previews
- A GitHub personal access token

## Installation

1. Clone this repository
2. Create a `.env` file in the project root with your GitHub token:
   ```
   GITHUB_TOKEN=your_github_token_here
   ```
3. Build the project:
   ```bash
   cargo build --release
   ```

## Usage

By default, the tool shows PRs for the current git repository:

```bash
cargo run --release
```

To list PRs from all your repositories:

```bash
cargo run --release -- --all
```

The tool will:
1. Fetch your PRs from GitHub (either from current repository or all repositories)
2. Display them in an interactive fzf interface
3. Show PR details with syntax-highlighted previews
4. Allow you to select a PR to view its details

## Features

- Lists PRs from current repository by default
- Optional `--all` flag to show PRs from all repositories
- Interactive fuzzy search interface
- Syntax-highlighted PR body previews
- Color-coded PR states (OPEN, CLOSED, DRAFT)
- Relative timestamps
- Sorted by status and update time

## Dependencies

The tool requires the following command-line utilities:
- `fzf` for the interactive interface
- `bat` for syntax highlighting in previews

Install them using your package manager:

```bash
# macOS
brew install fzf bat

# Ubuntu/Debian
sudo apt install fzf bat

# Arch Linux
sudo pacman -S fzf bat
```
