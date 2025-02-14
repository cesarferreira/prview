// listMyPRs.ts
import { Octokit } from "@octokit/rest";
import * as dotenv from "dotenv";

dotenv.config();

const GITHUB_TOKEN = process.env.GITHUB_TOKEN;
if (!GITHUB_TOKEN) {
  throw new Error("Missing GITHUB_TOKEN in environment variables");
}

const octokit = new Octokit({ auth: GITHUB_TOKEN });

async function listMyPRs(): Promise<void> {
  try {
    // Get the authenticated user's details
    const { data: user } = await octokit.rest.users.getAuthenticated();
    const username = user.login;
    console.log(`Authenticated as: ${username}`);

    // Build the search query for PRs authored by the user
    const query = `author:${username} is:pr`;

    // Search for issues/PRs with the query
    const { data: searchResults } = await octokit.rest.search.issuesAndPullRequests({
      q: query,
      per_page: 100, // adjust as needed
    });

    console.log(`Found ${searchResults.total_count} pull requests:`);
    searchResults.items.forEach(pr => {
      console.log(`${pr.title} - ${pr.html_url}`);
    });
  } catch (error) {
    console.error("Error listing PRs:", error);
  }
}

listMyPRs();