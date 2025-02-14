use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use colored::*;
use git2::Repository;
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

    /// Filter PRs by author (default: authenticated user)
    #[arg(long)]
    author: Option<String>,

    /// Disable preview panel
    #[arg(long)]
    no_preview: bool,
}

#[derive(Debug)]
struct PullRequest {
    number: i32,
    title: String,
    html_url: String,
    body: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    repository_name: String,
    state: String,
    is_draft: bool,
    merged: bool,
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

fn get_status_priority(pr: &PullRequest) -> i32 {
    if pr.is_draft || (pr.state == "OPEN" && !pr.is_draft) {
        0  // Highest priority for draft and open PRs
    } else if pr.merged {
        1  // Lower priority for merged
    } else {
        2  // Lowest priority for closed
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

async fn fetch_pull_requests(token: &str, owner: &str, repo: &str, author: &str) -> Result<Vec<PullRequest>> {
    let client = reqwest::Client::new();
    
    let query = r#"
    query($searchQuery: String!) {
      search(query: $searchQuery, type: ISSUE, first: 100) {
        nodes {
          ... on PullRequest {
            number
            title
            url
            body
            createdAt
            updatedAt
            isDraft
            state
            merged
            author {
              login
            }
            repository {
              nameWithOwner
            }
          }
        }
      }
    }
    "#;

    let search_query = format!("type:pr repo:{}/{} author:{}", owner, repo, author);
    
    let variables = serde_json::json!({
        "searchQuery": search_query,
    });

    let response = client
        .post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "rust-graphql-client")
        .json(&serde_json::json!({
            "query": query,
            "variables": variables,
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    if let Some(errors) = response.get("errors") {
        return Err(anyhow::anyhow!(
            "GraphQL Error: {}",
            serde_json::to_string_pretty(errors)?
        ));
    }

    let nodes = response["data"]["search"]["nodes"]
        .as_array()
        .context("No PRs found")?;

    let prs = nodes
        .iter()
        .map(|pr| {
            Ok(PullRequest {
                number: pr["number"].as_i64().context("No number")? as i32,
                title: pr["title"].as_str().context("No title")?.to_string(),
                html_url: pr["url"].as_str().context("No URL")?.to_string(),
                body: pr["body"].as_str().map(|s| s.to_string()),
                created_at: DateTime::parse_from_rfc3339(
                    pr["createdAt"].as_str().context("No createdAt")?,
                )?.with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(
                    pr["updatedAt"].as_str().context("No updatedAt")?,
                )?.with_timezone(&Utc),
                repository_name: pr["repository"]["nameWithOwner"].as_str().context("No repository name")?.to_string(),
                state: pr["state"].as_str().context("No state")?.to_string(),
                is_draft: pr["isDraft"].as_bool().context("No isDraft")?,
                merged: pr["merged"].as_bool().context("No merged status")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(prs)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let github_token = env::var("GITHUB_TOKEN")
        .context("Missing GITHUB_TOKEN in environment variables")?;

    let client = reqwest::Client::new();
    let author = if let Some(author) = args.author {
        author
    } else {
        let response = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {}", github_token))
            .header("User-Agent", "rust-graphql-client")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        response["login"]
            .as_str()
            .context("Could not get authenticated user")?
            .to_string()
    };

    let repo_info = get_current_repo_info()?
        .context("Not in a git repository or not a GitHub repository")?;

    let mut all_prs = fetch_pull_requests(&github_token, &repo_info.0, &repo_info.1, &author).await?;

    if all_prs.is_empty() {
        println!("No pull requests found.");
        return Ok(());
    }

    // Sort items by update time first, then by status priority
    all_prs.sort_by(|a, b| {
        let date_cmp = b.updated_at.cmp(&a.updated_at);  // Most recent first
        if date_cmp == std::cmp::Ordering::Equal {
            let pa = get_status_priority(a);
            let pb = get_status_priority(b);
            pa.cmp(&pb)
        } else {
            date_cmp
        }
    });

    // Create temporary directory
    let temp_dir = TempDir::new()?;

    let mut fzf_lines = Vec::new();
    let mut pr_map = Vec::new();

    for pr in &all_prs {
        let relative_time = get_relative_time(pr.updated_at);

        let status_colored = if pr.merged {
            "MERGED".purple().to_string()
        } else if pr.state == "CLOSED" {
            "CLOSED".red().to_string()
        } else if pr.is_draft {
            "DRAFT".dimmed().to_string()
        } else {
            "OPEN".green().to_string()
        };

        let title_colored = pr.title.blue().to_string();

        // Create PR body file
        let safe_repo_name = pr.repository_name.replace('/', "_");
        let file_name = format!("{}_{}.md", safe_repo_name, pr.number);
        let file_path = temp_dir.path().join(&file_name);

        if let Some(body) = &pr.body {
            fs::write(&file_path, body)?;
        } else {
            fs::write(&file_path, "")?;
        }

        pr_map.push((file_path.to_string_lossy().to_string(), pr));

        // Only include repository name in the display if --all flag is used
        let line = if args.all {
            format!(
                "{}\t{}\t{}\t{}\t{}",
                file_path.to_string_lossy(),
                relative_time,
                status_colored,
                title_colored,
                pr.repository_name
            )
        } else {
            format!(
                "{}\t{}\t{}\t{}",
                file_path.to_string_lossy(),
                relative_time,
                status_colored,
                title_colored,
            )
        };
        fzf_lines.push(line);
    }

    let fzf_input = fzf_lines.join("\n");

    // Create a temporary file for fzf input
    let mut input_file = tempfile::NamedTempFile::new()?;
    write!(input_file, "{}", fzf_input)?;

    // Adjust fzf command based on whether we're showing repository names and preview
    let preview_cmd = if args.no_preview {
        ""
    } else {
        "--preview 'bat --color=always --line-range :500 {1} | sed \"1d\"'"
    };

    let fzf_cmd = if args.all {
        format!(
            "fzf --ansi --delimiter='\t' --with-nth=2,3,4,5 {} < {}",
            preview_cmd,
            input_file.path().to_string_lossy()
        )
    } else {
        format!(
            "fzf --ansi --delimiter='\t' --with-nth=2,3,4 {} < {}",
            preview_cmd,
            input_file.path().to_string_lossy()
        )
    };

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
