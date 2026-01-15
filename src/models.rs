//! Data models for GitHub API responses.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/14/2026 - Initial implementation (Claude)

use serde::{Deserialize, Serialize};

/// GitHub user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: String,
    pub bio: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub website_url: Option<String>,
    pub twitter_username: Option<String>,
    pub public_repos: i32,
    pub followers: i32,
    pub following: i32,
    pub created_at: String,
}

/// GitHub repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub url: String,
    pub is_private: bool,
    pub is_fork: bool,
    pub stars: i32,
    pub forks: i32,
    pub language: Option<String>,
    pub updated_at: String,
    pub pushed_at: Option<String>,
}

/// GitHub issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub number: i32,
    pub title: String,
    pub state: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub comment_count: i32,
}

/// GitHub pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: i32,
    pub title: String,
    pub state: String,
    pub url: String,
    pub is_draft: bool,
    pub mergeable: String,
    pub created_at: String,
    pub updated_at: String,
    pub author: Option<String>,
    pub head_branch: String,
    pub base_branch: String,
    pub additions: i32,
    pub deletions: i32,
    pub changed_files: i32,
    pub commit_count: i32,
    pub comment_count: i32,
    pub reviews: Vec<Review>,
}

/// GitHub PR review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub author: Option<String>,
    pub state: String,
    pub submitted_at: Option<String>,
}

/// GitHub notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub unread: bool,
    pub reason: String,
    pub subject_title: String,
    pub subject_type: String,
    pub subject_url: Option<String>,
    pub repo_full_name: String,
    pub updated_at: String,
}

/// GraphQL response wrapper.
#[derive(Debug, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Option<Vec<GraphQLError>>,
}

/// GraphQL error.
#[derive(Debug, Deserialize)]
pub struct GraphQLError {
    pub message: String,
    #[serde(default)]
    pub path: Option<Vec<serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_serialization() {
        let user = User {
            login: "octocat".to_string(),
            name: Some("The Octocat".to_string()),
            email: Some("octocat@github.com".to_string()),
            avatar_url: "https://github.com/images/error/octocat.png".to_string(),
            bio: Some("A developer".to_string()),
            company: Some("@github".to_string()),
            location: Some("San Francisco".to_string()),
            website_url: Some("https://github.com/octocat".to_string()),
            twitter_username: Some("octocat".to_string()),
            public_repos: 42,
            followers: 1000,
            following: 10,
            created_at: "2008-01-14T04:33:35Z".to_string(),
        };

        let json = serde_json::to_string(&user).unwrap();
        let parsed: User = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.login, "octocat");
        assert_eq!(parsed.public_repos, 42);
    }

    #[test]
    fn test_repository_serialization() {
        let repo = Repository {
            name: "hello-world".to_string(),
            full_name: "octocat/hello-world".to_string(),
            description: Some("My first repo".to_string()),
            url: "https://github.com/octocat/hello-world".to_string(),
            is_private: false,
            is_fork: false,
            stars: 100,
            forks: 50,
            language: Some("Rust".to_string()),
            updated_at: "2024-01-14T00:00:00Z".to_string(),
            pushed_at: Some("2024-01-14T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&repo).unwrap();
        let parsed: Repository = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.full_name, "octocat/hello-world");
        assert_eq!(parsed.stars, 100);
    }

    #[test]
    fn test_issue_serialization() {
        let issue = Issue {
            number: 42,
            title: "Found a bug".to_string(),
            state: "OPEN".to_string(),
            url: "https://github.com/octocat/repo/issues/42".to_string(),
            created_at: "2024-01-14T00:00:00Z".to_string(),
            updated_at: "2024-01-14T00:00:00Z".to_string(),
            author: Some("octocat".to_string()),
            labels: vec!["bug".to_string(), "help wanted".to_string()],
            comment_count: 5,
        };

        let json = serde_json::to_string(&issue).unwrap();
        let parsed: Issue = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.number, 42);
        assert_eq!(parsed.labels.len(), 2);
    }

    #[test]
    fn test_pull_request_serialization() {
        let pr = PullRequest {
            number: 123,
            title: "Add new feature".to_string(),
            state: "OPEN".to_string(),
            url: "https://github.com/octocat/repo/pull/123".to_string(),
            is_draft: false,
            mergeable: "MERGEABLE".to_string(),
            created_at: "2024-01-14T00:00:00Z".to_string(),
            updated_at: "2024-01-14T00:00:00Z".to_string(),
            author: Some("octocat".to_string()),
            head_branch: "feature-branch".to_string(),
            base_branch: "main".to_string(),
            additions: 100,
            deletions: 50,
            changed_files: 5,
            commit_count: 3,
            comment_count: 2,
            reviews: vec![Review {
                author: Some("reviewer".to_string()),
                state: "APPROVED".to_string(),
                submitted_at: Some("2024-01-14T00:00:00Z".to_string()),
            }],
        };

        let json = serde_json::to_string(&pr).unwrap();
        let parsed: PullRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.number, 123);
        assert_eq!(parsed.reviews.len(), 1);
        assert_eq!(parsed.reviews[0].state, "APPROVED");
    }

    #[test]
    fn test_notification_serialization() {
        let notification = Notification {
            id: "12345".to_string(),
            unread: true,
            reason: "mention".to_string(),
            subject_title: "You were mentioned".to_string(),
            subject_type: "Issue".to_string(),
            subject_url: Some("https://api.github.com/repos/octocat/repo/issues/42".to_string()),
            repo_full_name: "octocat/repo".to_string(),
            updated_at: "2024-01-14T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&notification).unwrap();
        let parsed: Notification = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "12345");
        assert!(parsed.unread);
    }
}
