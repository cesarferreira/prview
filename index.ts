// listMyPRs.ts
import { Octokit } from "@octokit/rest";
import * as dotenv from "dotenv";

dotenv.config();

const GITHUB_TOKEN = process.env.GITHUB_TOKEN;
if (!GITHUB_TOKEN) {
  throw new Error("Missing GITHUB_TOKEN in environment variables");
}

const octokit = new Octokit({ auth: GITHUB_TOKEN });

// Define an interface for the search result items
interface SearchItem {
  title: string;
  html_url: string;
  created_at: string;
  updated_at: string; // Last updated timestamp
  repository_url: string;
  state: "open" | "closed";
  draft?: boolean;
}

async function listMyPRs(): Promise<void> {
  try {
    // Get the authenticated user's details
    const { data: user } = await octokit.rest.users.getAuthenticated();
    const username = user.login;
    console.log(`Authenticated as: ${username}\n`);

    // Build the search query for PRs authored by the user
    const query = `author:${username} is:pr`;

    // Search for issues/PRs with the query
    const { data: searchResults } = await octokit.rest.search.issuesAndPullRequests({
      q: query,
      per_page: 100, // adjust as needed
    });

    let items = searchResults.items as SearchItem[];

    // Function to determine sort priority based on PR status.
    // Lower number means higher priority.
    const getStatusPriority = (pr: SearchItem): number => {
      if (pr.draft) return 0;         // Drafts: highest priority
      if (pr.state === "open") return 1;  // Open PRs: next
      if (pr.state === "closed") return 2; // Closed PRs: last
      return 3; // fallback (if any)
    };

    // Sort items: first by status priority, then by last updated date (most recent changes on top)
    items.sort((a, b) => {
      const statusPriorityA = getStatusPriority(a);
      const statusPriorityB = getStatusPriority(b);

      if (statusPriorityA !== statusPriorityB) {
        return statusPriorityA - statusPriorityB;
      }

      // Both PRs have the same status, sort by updated_at descending (most recent first)
      return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
    });

    console.log(`Found ${searchResults.total_count} pull requests:\n`);

    items.forEach(pr => {
      // Extract repository (project) name from repository_url.
      // Example: "https://api.github.com/repos/owner/repo" becomes "owner/repo"
      const projectName = pr.repository_url.replace("https://api.github.com/repos/", "");

      // Determine the status label.
      // If pr.draft is true, label it as "DRAFT", else show its state.
      const statusLabel = pr.draft ? "DRAFT" : pr.state.toUpperCase();

      console.log(`Title         : ${pr.title}`);
      console.log(`URL           : ${pr.html_url}`);
      console.log(`Created At    : ${pr.created_at}`);
      console.log(`Last Updated  : ${pr.updated_at}`);
      console.log(`Repository    : ${projectName}`);
      console.log(`Status        : ${statusLabel}`);
      console.log(`----------------------------------`);
    });
  } catch (error) {
    console.error("Error listing PRs:", error);
  }
}

listMyPRs();