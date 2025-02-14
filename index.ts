// listMyPRs.ts
import { Octokit } from "@octokit/rest";
import * as dotenv from "dotenv";
import { spawnSync } from "child_process";
import { mkdtempSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

dotenv.config();

const GITHUB_TOKEN = process.env.GITHUB_TOKEN;
if (!GITHUB_TOKEN) {
  throw new Error("Missing GITHUB_TOKEN in environment variables");
}

const octokit = new Octokit({ auth: GITHUB_TOKEN });

interface SearchItem {
  number: number;
  title: string;
  html_url: string;
  body: string;
  created_at: string;
  updated_at: string;
  repository_url: string;
  state: "open" | "closed";
  draft?: boolean;
}

/**
 * Returns a human-readable relative time.
 * Examples: "5 minutes ago", "2 hours ago", "1 day ago", "3 weeks ago"
 */
function getRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = diffMs / 1000;
  const diffMin = diffSec / 60;
  const diffHour = diffMin / 60;
  const diffDay = diffHour / 24;
  const diffWeek = diffDay / 7;

  if (diffDay < 1) {
    if (diffHour < 1) {
      return `${Math.floor(diffMin)} minute${Math.floor(diffMin) === 1 ? "" : "s"} ago`;
    } else {
      return `${Math.floor(diffHour)} hour${Math.floor(diffHour) === 1 ? "" : "s"} ago`;
    }
  } else if (diffDay < 7) {
    const days = Math.floor(diffDay);
    return `${days} day${days === 1 ? "" : "s"} ago`;
  } else {
    const weeks = Math.floor(diffWeek);
    return `${weeks} week${weeks === 1 ? "" : "s"} ago`;
  }
}

async function listMyPRs(): Promise<void> {
  try {
    // Get authenticated user details.
    const { data: user } = await octokit.rest.users.getAuthenticated();
    const username = user.login;
    console.log(`Authenticated as: ${username}\n`);

    // Build search query for PRs authored by the user.
    const query = `author:${username} is:pr`;
    const { data: searchResults } = await octokit.rest.search.issuesAndPullRequests({
      q: query,
      per_page: 100,
    });
    let items = searchResults.items as SearchItem[];

    // Sorting:
    // - If the PR is closed, it should always come with priority 2.
    // - Otherwise, if it's a draft, priority 0.
    // - Then open PRs get priority 1.
    const getStatusPriority = (pr: SearchItem): number => {
      if (pr.state === "closed") return 2;
      if (pr.draft) return 0;
      if (pr.state === "open") return 1;
      return 3;
    };

    items.sort((a, b) => {
      const pA = getStatusPriority(a);
      const pB = getStatusPriority(b);
      if (pA !== pB) return pA - pB;
      return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
    });

    // Create a temporary directory to hold PR body files.
    const tempDir = mkdtempSync(join(tmpdir(), "pr-bodies-"));

    // Map the preview file path to its PR.
    const prMap = new Map<string, SearchItem>();

    // ANSI escape codes.
    const blue = "\x1b[34m";
    const green = "\x1b[32m";
    const red = "\x1b[31m";
    const grey = "\x1b[90m";
    const reset = "\x1b[0m";

    // Build fzf input lines (tab-separated):
    // Field 1: preview file path (hidden)
    // Field 2: Relative time (plain text, on the left)
    // Field 3: Status (colored: DRAFT in grey, OPEN in green, CLOSED in red)
    // Field 4: PR title (colored blue)
    // Field 5: Project name (owner/repo)
    const fzfLines = items.map(pr => {
      const repoName = pr.repository_url.replace("https://api.github.com/repos/", "");
      const relativeTime = getRelativeTime(pr.updated_at);
      
      let statusColored: string;
      if (pr.state === "closed") {
        statusColored = `${red}CLOSED${reset}`;
      } else if (pr.draft) {
        statusColored = `${grey}DRAFT${reset}`;
      } else {
        statusColored = `${green}OPEN${reset}`;
      }
      
      const titleColored = `${blue}${pr.title}${reset}`;

      // Create a file for the PR body (markdown).
      const safeRepoName = repoName.replace(/\//g, "_");
      const fileName = `${safeRepoName}_${pr.number}.md`;
      const filePath = join(tempDir, fileName);
      writeFileSync(filePath, pr.body || "", "utf-8");

      prMap.set(filePath, pr);

      // Visible fields order: Relative time, Status, Title, Project name.
      return `${filePath}\t${relativeTime}\t${statusColored}\t${titleColored}\t${repoName}`;
    });
    const fzfInput = fzfLines.join("\n");

    // fzf command:
    // --ansi to enable ANSI color codes.
    // --delimiter sets tab as separator.
    // --with-nth=2,3,4,5 displays Relative time, Status, Title, and Project name.
    // The preview uses field {1} (the preview file path) and pipes it to bat.
    const fzfCmd = `fzf --ansi --delimiter="\t" --with-nth=2,3,4,5 --preview 'bat --style=numbers --color=always --line-range :500 {1}'`;

    const fzfResult = spawnSync(fzfCmd, {
      input: fzfInput,
      encoding: "utf-8",
      shell: true,
    });

    // Clean up the temporary directory.
    rmSync(tempDir, { recursive: true, force: true });

    const selectedLine = fzfResult.stdout.trim();
    if (!selectedLine) {
      console.log("No PR selected.");
      return;
    }

    const selectedFields = selectedLine.split("\t");
    const previewFilePath = selectedFields[0];
    const selectedPR = prMap.get(previewFilePath);
    if (selectedPR) {
      console.log("\nSelected PR:");
      console.log(`Title: ${selectedPR.title}`);
      console.log(`URL  : ${selectedPR.html_url}`);
    }
  } catch (error) {
    console.error("Error listing PRs:", error);
  }
}

listMyPRs();