//! GitHub GraphQL and REST API client with connection pooling.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/14/2026 - Initial implementation with GraphQL + REST (Claude)

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::models::{GraphQLResponse, Issue, Notification, PullRequest, Repository, User};

const GRAPHQL_ENDPOINT: &str = "https://api.github.com/graphql";
const REST_ENDPOINT: &str = "https://api.github.com";

/// GitHub API client with persistent connection pooling.
pub struct GitHubClient {
    client: Client,
    token: String,
}

impl GitHubClient {
    /// Create a new GitHub client.
    ///
    /// Token resolution order:
    /// 1. Explicit token parameter
    /// 2. GITHUB_TOKEN environment variable
    /// 3. gh CLI config (~/.config/gh/hosts.yml)
    pub fn new(token: Option<String>) -> Result<Self> {
        let token = match token {
            Some(t) => t,
            None => Self::resolve_token()?,
        };

        let client = Client::builder()
            .pool_max_idle_per_host(5)
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("fgp-github/0.2.0")
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { client, token })
    }

    /// Resolve GitHub token from environment or gh CLI config.
    fn resolve_token() -> Result<String> {
        // Try GITHUB_TOKEN env var first
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.is_empty() {
                return Ok(token);
            }
        }

        // Try GH_TOKEN (alternative env var used by gh CLI)
        if let Ok(token) = std::env::var("GH_TOKEN") {
            if !token.is_empty() {
                return Ok(token);
            }
        }

        // Fall back to gh CLI config
        Self::read_gh_token()
    }

    /// Read token from gh CLI config file.
    fn read_gh_token() -> Result<String> {
        let config_path = Self::gh_config_path()?;

        if !config_path.exists() {
            bail!(
                "No GitHub token found. Set GITHUB_TOKEN env var or run 'gh auth login'.\n\
                 Config path checked: {}",
                config_path.display()
            );
        }

        let content =
            std::fs::read_to_string(&config_path).context("Failed to read gh config file")?;

        // Parse YAML config
        let config: Value = serde_yaml::from_str(&content).context("Failed to parse gh config")?;

        // Extract token for github.com
        let token = config
            .get("github.com")
            .and_then(|host| host.get("oauth_token"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        token.ok_or_else(|| {
            anyhow::anyhow!(
                "No oauth_token found for github.com in {}",
                config_path.display()
            )
        })
    }

    /// Get gh CLI config path.
    fn gh_config_path() -> Result<PathBuf> {
        // Check XDG_CONFIG_HOME first
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg).join("gh").join("hosts.yml"));
        }

        // Fall back to ~/.config/gh/hosts.yml
        let home = dirs::home_dir().context("Could not determine home directory")?;

        Ok(home.join(".config").join("gh").join("hosts.yml"))
    }

    /// Execute a GraphQL query.
    async fn graphql<T: for<'de> Deserialize<'de>>(
        &self,
        query: &str,
        variables: Option<Value>,
    ) -> Result<T> {
        let body = GraphQLRequest {
            query: query.to_string(),
            variables,
        };

        let response = self
            .client
            .post(GRAPHQL_ENDPOINT)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()
            .await
            .context("Failed to send GraphQL request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("GraphQL request failed: {} - {}", status, text);
        }

        let text = response.text().await.context("Failed to read response")?;

        let result: GraphQLResponse<T> = serde_json::from_str(&text).map_err(|e| {
            anyhow::anyhow!(
                "JSON parse error: {} | Raw: {}",
                e,
                &text[..text.len().min(500)]
            )
        })?;

        // Check for GraphQL errors
        if result.data.is_none() {
            if let Some(errors) = result.errors {
                if !errors.is_empty() {
                    let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
                    bail!("GraphQL errors: {}", messages.join(", "));
                }
            }
        }

        result.data.context("GraphQL response missing data field")
    }

    /// Execute a REST API request (GET).
    async fn rest_get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", REST_ENDPOINT, path);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("Failed to send REST request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("REST request failed: {} - {}", status, text);
        }

        let result = response.json().await.context("Failed to parse JSON")?;
        Ok(result)
    }

    /// Check if the client can connect to GitHub API.
    pub async fn ping(&self) -> Result<bool> {
        let query = r#"
            query {
                viewer {
                    login
                }
            }
        "#;

        #[derive(Deserialize)]
        struct ViewerResponse {
            viewer: Viewer,
        }

        #[derive(Deserialize)]
        struct Viewer {
            login: String,
        }

        let result: ViewerResponse = self.graphql(query, None).await?;
        Ok(!result.viewer.login.is_empty())
    }

    /// Get current authenticated user.
    /// Note: email field requires 'user:email' or 'read:user' scope.
    /// If token lacks these scopes, email will be None.
    pub async fn get_user(&self) -> Result<User> {
        // First try with email (requires user:email scope)
        let query_with_email = r#"
            query {
                viewer {
                    login
                    name
                    email
                    avatarUrl
                    bio
                    company
                    location
                    websiteUrl
                    twitterUsername
                    repositories {
                        totalCount
                    }
                    followers {
                        totalCount
                    }
                    following {
                        totalCount
                    }
                    createdAt
                }
            }
        "#;

        // Fallback without email (works with basic scopes)
        let query_without_email = r#"
            query {
                viewer {
                    login
                    name
                    avatarUrl
                    bio
                    company
                    location
                    websiteUrl
                    twitterUsername
                    repositories {
                        totalCount
                    }
                    followers {
                        totalCount
                    }
                    following {
                        totalCount
                    }
                    createdAt
                }
            }
        "#;

        #[derive(Deserialize)]
        struct ViewerResponse {
            viewer: ViewerData,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ViewerData {
            login: String,
            name: Option<String>,
            #[serde(default)]
            email: Option<String>,
            avatar_url: String,
            bio: Option<String>,
            company: Option<String>,
            location: Option<String>,
            website_url: Option<String>,
            twitter_username: Option<String>,
            repositories: CountWrapper,
            followers: CountWrapper,
            following: CountWrapper,
            created_at: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CountWrapper {
            total_count: i32,
        }

        // Try with email first, fall back to without if scope error
        let result: Result<ViewerResponse> = self.graphql(query_with_email, None).await;

        let v = match result {
            Ok(r) => r.viewer,
            Err(e)
                if e.to_string().contains("user:email") || e.to_string().contains("read:user") =>
            {
                // Token lacks email scope, try without
                let r: ViewerResponse = self.graphql(query_without_email, None).await?;
                r.viewer
            }
            Err(e) => return Err(e),
        };

        Ok(User {
            login: v.login,
            name: v.name,
            email: v.email,
            avatar_url: v.avatar_url,
            bio: v.bio,
            company: v.company,
            location: v.location,
            website_url: v.website_url,
            twitter_username: v.twitter_username,
            public_repos: v.repositories.total_count,
            followers: v.followers.total_count,
            following: v.following.total_count,
            created_at: v.created_at,
        })
    }

    /// List user's repositories.
    pub async fn list_repos(&self, limit: i32) -> Result<Vec<Repository>> {
        let query = r#"
            query($first: Int!) {
                viewer {
                    repositories(first: $first, orderBy: {field: UPDATED_AT, direction: DESC}) {
                        nodes {
                            name
                            nameWithOwner
                            description
                            url
                            isPrivate
                            isFork
                            stargazerCount
                            forkCount
                            primaryLanguage {
                                name
                            }
                            updatedAt
                            pushedAt
                        }
                    }
                }
            }
        "#;

        #[derive(Deserialize)]
        struct ViewerResponse {
            viewer: ViewerRepos,
        }

        #[derive(Deserialize)]
        struct ViewerRepos {
            repositories: RepoNodes,
        }

        #[derive(Deserialize)]
        struct RepoNodes {
            nodes: Vec<RepoNode>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RepoNode {
            name: String,
            name_with_owner: String,
            description: Option<String>,
            url: String,
            is_private: bool,
            is_fork: bool,
            stargazer_count: i32,
            fork_count: i32,
            primary_language: Option<LanguageNode>,
            updated_at: String,
            pushed_at: Option<String>,
        }

        #[derive(Deserialize)]
        struct LanguageNode {
            name: String,
        }

        let variables = serde_json::json!({ "first": limit });
        let result: ViewerResponse = self.graphql(query, Some(variables)).await?;

        let repos = result
            .viewer
            .repositories
            .nodes
            .into_iter()
            .map(|n| Repository {
                name: n.name,
                full_name: n.name_with_owner,
                description: n.description,
                url: n.url,
                is_private: n.is_private,
                is_fork: n.is_fork,
                stars: n.stargazer_count,
                forks: n.fork_count,
                language: n.primary_language.map(|l| l.name),
                updated_at: n.updated_at,
                pushed_at: n.pushed_at,
            })
            .collect();

        Ok(repos)
    }

    /// List issues for a repository.
    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: &str,
        limit: i32,
    ) -> Result<Vec<Issue>> {
        let states = match state.to_uppercase().as_str() {
            "OPEN" => "[OPEN]",
            "CLOSED" => "[CLOSED]",
            "ALL" => "[OPEN, CLOSED]",
            _ => "[OPEN]",
        };

        let query = format!(
            r#"
            query($owner: String!, $name: String!, $first: Int!) {{
                repository(owner: $owner, name: $name) {{
                    issues(first: $first, states: {}, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{
                        nodes {{
                            number
                            title
                            state
                            url
                            createdAt
                            updatedAt
                            author {{
                                login
                            }}
                            labels(first: 10) {{
                                nodes {{
                                    name
                                    color
                                }}
                            }}
                            comments {{
                                totalCount
                            }}
                        }}
                    }}
                }}
            }}
        "#,
            states
        );

        #[derive(Deserialize)]
        struct RepoResponse {
            repository: RepoData,
        }

        #[derive(Deserialize)]
        struct RepoData {
            issues: IssueNodes,
        }

        #[derive(Deserialize)]
        struct IssueNodes {
            nodes: Vec<IssueNode>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueNode {
            number: i32,
            title: String,
            state: String,
            url: String,
            created_at: String,
            updated_at: String,
            author: Option<AuthorNode>,
            labels: LabelNodes,
            comments: CommentCount,
        }

        #[derive(Deserialize)]
        struct AuthorNode {
            login: String,
        }

        #[derive(Deserialize)]
        struct LabelNodes {
            nodes: Vec<LabelNode>,
        }

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct LabelNode {
            name: String,
            color: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CommentCount {
            total_count: i32,
        }

        let variables = serde_json::json!({
            "owner": owner,
            "name": repo,
            "first": limit
        });

        let result: RepoResponse = self.graphql(&query, Some(variables)).await?;

        let issues = result
            .repository
            .issues
            .nodes
            .into_iter()
            .map(|n| Issue {
                number: n.number,
                title: n.title,
                state: n.state,
                url: n.url,
                created_at: n.created_at,
                updated_at: n.updated_at,
                author: n.author.map(|a| a.login),
                labels: n.labels.nodes.into_iter().map(|l| l.name).collect(),
                comment_count: n.comments.total_count,
            })
            .collect();

        Ok(issues)
    }

    /// Get unread notifications.
    pub async fn get_notifications(&self) -> Result<Vec<Notification>> {
        // Use REST API for notifications (simpler)
        let notifications: Vec<NotificationRaw> = self.rest_get("/notifications").await?;

        let result = notifications
            .into_iter()
            .map(|n| Notification {
                id: n.id,
                unread: n.unread,
                reason: n.reason,
                subject_title: n.subject.title,
                subject_type: n.subject.type_field,
                subject_url: n.subject.url,
                repo_full_name: n.repository.full_name,
                updated_at: n.updated_at,
            })
            .collect();

        Ok(result)
    }

    /// Get pull request details with status checks and reviews.
    pub async fn get_pr(&self, owner: &str, repo: &str, pr_number: i32) -> Result<PullRequest> {
        let query = r#"
            query($owner: String!, $name: String!, $number: Int!) {
                repository(owner: $owner, name: $name) {
                    pullRequest(number: $number) {
                        number
                        title
                        state
                        url
                        isDraft
                        mergeable
                        createdAt
                        updatedAt
                        author {
                            login
                        }
                        headRefName
                        baseRefName
                        additions
                        deletions
                        changedFiles
                        commits {
                            totalCount
                        }
                        comments {
                            totalCount
                        }
                        reviews(first: 10) {
                            nodes {
                                author {
                                    login
                                }
                                state
                                submittedAt
                            }
                        }
                        commits(last: 1) {
                            nodes {
                                commit {
                                    statusCheckRollup {
                                        state
                                        contexts(first: 20) {
                                            nodes {
                                                ... on CheckRun {
                                                    name
                                                    status
                                                    conclusion
                                                }
                                                ... on StatusContext {
                                                    context
                                                    state
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        "#;

        #[derive(Deserialize)]
        struct RepoResponse {
            repository: RepoData,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RepoData {
            pull_request: PullRequestNode,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PullRequestNode {
            number: i32,
            title: String,
            state: String,
            url: String,
            is_draft: bool,
            mergeable: String,
            created_at: String,
            updated_at: String,
            author: Option<AuthorNode>,
            head_ref_name: String,
            base_ref_name: String,
            additions: i32,
            deletions: i32,
            changed_files: i32,
            commits: CommitCount,
            comments: CommentCount,
            reviews: ReviewNodes,
        }

        #[derive(Deserialize)]
        struct AuthorNode {
            login: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CommitCount {
            total_count: i32,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CommentCount {
            total_count: i32,
        }

        #[derive(Deserialize)]
        struct ReviewNodes {
            nodes: Vec<ReviewNode>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ReviewNode {
            author: Option<AuthorNode>,
            state: String,
            submitted_at: Option<String>,
        }

        let variables = serde_json::json!({
            "owner": owner,
            "name": repo,
            "number": pr_number
        });

        let result: RepoResponse = self.graphql(query, Some(variables)).await?;
        let pr = result.repository.pull_request;

        let reviews = pr
            .reviews
            .nodes
            .into_iter()
            .map(|r| crate::models::Review {
                author: r.author.map(|a| a.login),
                state: r.state,
                submitted_at: r.submitted_at,
            })
            .collect();

        Ok(PullRequest {
            number: pr.number,
            title: pr.title,
            state: pr.state,
            url: pr.url,
            is_draft: pr.is_draft,
            mergeable: pr.mergeable,
            created_at: pr.created_at,
            updated_at: pr.updated_at,
            author: pr.author.map(|a| a.login),
            head_branch: pr.head_ref_name,
            base_branch: pr.base_ref_name,
            additions: pr.additions,
            deletions: pr.deletions,
            changed_files: pr.changed_files,
            commit_count: pr.commits.total_count,
            comment_count: pr.comments.total_count,
            reviews,
        })
    }

    /// List pull requests for a repository.
    pub async fn list_prs(
        &self,
        owner: &str,
        repo: &str,
        state: &str,
        limit: i32,
    ) -> Result<Vec<PullRequest>> {
        let states = match state.to_uppercase().as_str() {
            "OPEN" => "[OPEN]",
            "CLOSED" => "[CLOSED]",
            "MERGED" => "[MERGED]",
            "ALL" => "[OPEN, CLOSED, MERGED]",
            _ => "[OPEN]",
        };

        let query = format!(
            r#"
            query($owner: String!, $name: String!, $first: Int!) {{
                repository(owner: $owner, name: $name) {{
                    pullRequests(first: $first, states: {}, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{
                        nodes {{
                            number
                            title
                            state
                            url
                            isDraft
                            mergeable
                            createdAt
                            updatedAt
                            author {{
                                login
                            }}
                            headRefName
                            baseRefName
                            additions
                            deletions
                            changedFiles
                            commits {{
                                totalCount
                            }}
                            comments {{
                                totalCount
                            }}
                            reviews(first: 5) {{
                                nodes {{
                                    author {{
                                        login
                                    }}
                                    state
                                    submittedAt
                                }}
                            }}
                        }}
                    }}
                }}
            }}
        "#,
            states
        );

        #[derive(Deserialize)]
        struct RepoResponse {
            repository: RepoData,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RepoData {
            pull_requests: PrNodes,
        }

        #[derive(Deserialize)]
        struct PrNodes {
            nodes: Vec<PrNode>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PrNode {
            number: i32,
            title: String,
            state: String,
            url: String,
            is_draft: bool,
            mergeable: String,
            created_at: String,
            updated_at: String,
            author: Option<AuthorNode>,
            head_ref_name: String,
            base_ref_name: String,
            additions: i32,
            deletions: i32,
            changed_files: i32,
            commits: CommitCount,
            comments: CommentCount,
            reviews: ReviewNodes,
        }

        #[derive(Deserialize)]
        struct AuthorNode {
            login: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CommitCount {
            total_count: i32,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CommentCount {
            total_count: i32,
        }

        #[derive(Deserialize)]
        struct ReviewNodes {
            nodes: Vec<ReviewNode>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ReviewNode {
            author: Option<AuthorNode>,
            state: String,
            submitted_at: Option<String>,
        }

        let variables = serde_json::json!({
            "owner": owner,
            "name": repo,
            "first": limit
        });

        let result: RepoResponse = self.graphql(&query, Some(variables)).await?;

        let prs = result
            .repository
            .pull_requests
            .nodes
            .into_iter()
            .map(|pr| {
                let reviews = pr
                    .reviews
                    .nodes
                    .into_iter()
                    .map(|r| crate::models::Review {
                        author: r.author.map(|a| a.login),
                        state: r.state,
                        submitted_at: r.submitted_at,
                    })
                    .collect();

                PullRequest {
                    number: pr.number,
                    title: pr.title,
                    state: pr.state,
                    url: pr.url,
                    is_draft: pr.is_draft,
                    mergeable: pr.mergeable,
                    created_at: pr.created_at,
                    updated_at: pr.updated_at,
                    author: pr.author.map(|a| a.login),
                    head_branch: pr.head_ref_name,
                    base_branch: pr.base_ref_name,
                    additions: pr.additions,
                    deletions: pr.deletions,
                    changed_files: pr.changed_files,
                    commit_count: pr.commits.total_count,
                    comment_count: pr.comments.total_count,
                    reviews,
                }
            })
            .collect();

        Ok(prs)
    }

    /// Create an issue.
    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<Issue> {
        let query = r#"
            mutation($repositoryId: ID!, $title: String!, $body: String) {
                createIssue(input: {repositoryId: $repositoryId, title: $title, body: $body}) {
                    issue {
                        number
                        title
                        state
                        url
                        createdAt
                        updatedAt
                        author {
                            login
                        }
                    }
                }
            }
        "#;

        // First, get the repository ID
        let repo_id = self.get_repo_id(owner, repo).await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CreateIssueResponse {
            create_issue: CreateIssueData,
        }

        #[derive(Deserialize)]
        struct CreateIssueData {
            issue: IssueNode,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueNode {
            number: i32,
            title: String,
            state: String,
            url: String,
            created_at: String,
            updated_at: String,
            author: Option<AuthorNode>,
        }

        #[derive(Deserialize)]
        struct AuthorNode {
            login: String,
        }

        let variables = serde_json::json!({
            "repositoryId": repo_id,
            "title": title,
            "body": body
        });

        let result: CreateIssueResponse = self.graphql(query, Some(variables)).await?;
        let issue = result.create_issue.issue;

        Ok(Issue {
            number: issue.number,
            title: issue.title,
            state: issue.state,
            url: issue.url,
            created_at: issue.created_at,
            updated_at: issue.updated_at,
            author: issue.author.map(|a| a.login),
            labels: vec![],
            comment_count: 0,
        })
    }

    /// Get repository node ID (needed for mutations).
    async fn get_repo_id(&self, owner: &str, repo: &str) -> Result<String> {
        let query = r#"
            query($owner: String!, $name: String!) {
                repository(owner: $owner, name: $name) {
                    id
                }
            }
        "#;

        #[derive(Deserialize)]
        struct RepoResponse {
            repository: RepoId,
        }

        #[derive(Deserialize)]
        struct RepoId {
            id: String,
        }

        let variables = serde_json::json!({
            "owner": owner,
            "name": repo
        });

        let result: RepoResponse = self.graphql(query, Some(variables)).await?;
        Ok(result.repository.id)
    }
}

/// GraphQL request body.
#[derive(Serialize)]
struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<Value>,
}

/// Raw notification from REST API.
#[derive(Deserialize)]
struct NotificationRaw {
    id: String,
    unread: bool,
    reason: String,
    subject: NotificationSubject,
    repository: NotificationRepo,
    updated_at: String,
}

#[derive(Deserialize)]
struct NotificationSubject {
    title: String,
    #[serde(rename = "type")]
    type_field: String,
    url: Option<String>,
}

#[derive(Deserialize)]
struct NotificationRepo {
    full_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gh_config_path() {
        let path = GitHubClient::gh_config_path().unwrap();
        assert!(path.to_string_lossy().contains("gh/hosts.yml"));
    }
}
