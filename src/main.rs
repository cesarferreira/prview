use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use colored::*;
use git2::Repository;
use octocrab::models::pulls::PullRequest;
use serde::Deserialize;
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List PRs from all repositories (default: only current repository)
    #[arg(long)]
    all: bool,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    number: i32,
    title: String,
    html_url: String,
    body: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    repository_url: String,
    state: String,
    draft: Option<bool>,
}

fn get_relative_time(date: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(date);

    let diff_mins = diff.num_minutes();
    let diff_hours = diff.num_hours();
    let diff_days = diff.num_days();
    let diff_weeks = diff_days / 7;

    if diff_days < 1 {
        if diff_hours < 1 {
            format!("{} minute{} ago", diff_mins, if diff_mins == 1 { "" } else { "s" })
        } else {
            format!("{} hour{} ago", diff_hours, if diff_hours == 1 { "" } else { "s" })
        }
    } else if diff_days < 7 {
        format!("{} day{} ago", diff_days, if diff_days == 1 { "" } else { "s" })
    } else {
        format!("{} week{} ago", diff_weeks, if diff_weeks == 1 { "" } else { "s" })
    }
}

fn get_status_priority(pr: &SearchItem) -> i32 {
    if pr.state == "closed" {
        2
    } else if pr.draft.unwrap_or(false) {
        0
    } else if pr.state == "open" {
        1
    } else {
        3
    }
}

fn get_current_repo_info() -> Result<Option<(String, String)>> {
    let current_dir = env::current_dir()?;
    let repo = match Repository::discover(&current_dir) {
        Ok(repo) => repo,
        Err(_) => return Ok(None),
    };

    let remote = repo
        .find_remote("origin")
        .context("No 'origin' remote found")?;
    
    let url = remote.url().context("No URL found for origin remote")?;
    
    // Extract owner and repo from different URL formats
    let repo_path = if url.contains("github.com:") {
        // SSH format: git@github.com:owner/repo.git
        url.split("github.com:").nth(1)
    } else if url.contains("github.com/") {
        // HTTPS format: https://github.com/owner/repo.git
        url.split("github.com/").nth(1)
    } else {
        return Err(anyhow::anyhow!("Not a GitHub repository URL: {}", url));
    }
    .context("Could not parse GitHub repository URL")?
    .trim_end_matches(".git")
    .to_string();

    // Split into owner and repo
    let parts: Vec<&str> = repo_path.split('/').collect();
    if parts.len() >= 2 {
        Ok(Some((parts[0].to_string(), parts[1].to_string())))
    } else {
        Err(anyhow::anyhow!("Invalid GitHub repository format: {}", repo_path))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let github_token = env::var("GITHUB_TOKEN")
        .context("Missing GITHUB_TOKEN in environment variables")?;

    let octocrab = octocrab::Octocrab::builder()
        .personal_token(github_token)
        .build()
        .context("Failed to create GitHub client")?;

    // Get authenticated user
    let user = octocrab.current().user().await?;
    println!("Authenticated as: {}\n", user.login);

    // Build search query based on arguments
    let query = if args.all {
        format!("author:{} is:pr", user.login)
    } else {
        // Get current repository information
        let repo_info = get_current_repo_info()?
            .context("Not in a git repository or not a GitHub repository")?;
        format!("author:{} is:pr repo:{}/{}", user.login, repo_info.0, repo_info.1)
    };

    let search_results = octocrab
        .search()
        .issues_and_pull_requests(&query)
        .per_page(100)
        .send()
        .await?;

    let mut items: Vec<SearchItem> = serde_json::from_value(serde_json::to_value(search_results.items)?)?;

    if items.is_empty() {
        println!("No pull requests found.");
        return Ok(());
    }

    // Sort items
    items.sort_by(|a, b| {
        let pa = get_status_priority(a);
        let pb = get_status_priority(b);
        if pa != pb {
            pa.cmp(&pb)
        } else {
            b.updated_at.cmp(&a.updated_at)
        }
    });

    // Create temporary directory
    let temp_dir = TempDir::new()?;

    let mut fzf_lines = Vec::new();
    let mut pr_map = Vec::new();

    for pr in &items {
        let repo_name = pr.repository_url.replace("https://api.github.com/repos/", "");
        let relative_time = get_relative_time(pr.updated_at);

        let status_colored = if pr.state == "closed" {
            "CLOSED".red().to_string()
        } else if pr.draft.unwrap_or(false) {
            "DRAFT".dimmed().to_string()
        } else {
            "OPEN".green().to_string()
        };

        let title_colored = pr.title.blue().to_string();

        // Create PR body file
        let safe_repo_name = repo_name.replace('/', "_");
        let file_name = format!("{}_{}.md", safe_repo_name, pr.number);
        let file_path = temp_dir.path().join(&file_name);

        if let Some(body) = &pr.body {
            fs::write(&file_path, body)?;
        } else {
            fs::write(&file_path, "")?;
        }

        pr_map.push((file_path.to_string_lossy().to_string(), pr));

        let line = format!(
            "{}\t{}\t{}\t{}\t{}",
            file_path.to_string_lossy(),
            relative_time,
            status_colored,
            title_colored,
            repo_name
        );
        fzf_lines.push(line);
    }

    let fzf_input = fzf_lines.join("\n");

    // Create a temporary file for fzf input
    let mut input_file = tempfile::NamedTempFile::new()?;
    write!(input_file, "{}", fzf_input)?;

    // Run fzf
    let fzf_cmd = format!(
        "fzf --ansi --delimiter='\t' --with-nth=2,3,4,5 --preview 'bat --color=always --line-range :500 {{1}}' < {}",
        input_file.path().to_string_lossy()
    );

    let output = duct::cmd!("sh", "-c", &fzf_cmd)
        .stdin_null()
        .stdout_capture()
        .unchecked()
        .run()?;

    if !output.stdout.is_empty() {
        let selected = String::from_utf8(output.stdout)?;
        let selected_fields: Vec<&str> = selected.trim().split('\t').collect();
        let preview_file_path = selected_fields[0];

        if let Some((_, pr)) = pr_map.iter().find(|(path, _)| path == preview_file_path) {
            println!("\nSelected PR:");
            println!("Title: {}", pr.title);
            println!("URL  : {}", pr.html_url);
        }
    } else {
        println!("No PR selected.");
    }

    Ok(())
}
