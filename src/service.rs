//! FGP service implementation for GitHub.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/14/2026 - Initial implementation with GraphQL/REST (Claude)

use anyhow::Result;
use fgp_daemon::service::{HealthStatus, MethodInfo, ParamInfo};
use fgp_daemon::FgpService;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

use crate::api::GitHubClient;

/// FGP service for GitHub operations.
pub struct GitHubService {
    client: Arc<GitHubClient>,
    runtime: Runtime,
}

impl GitHubService {
    /// Create a new GitHubService.
    ///
    /// Token is resolved from:
    /// 1. GITHUB_TOKEN environment variable
    /// 2. gh CLI config (~/.config/gh/hosts.yml)
    pub fn new(token: Option<String>) -> Result<Self> {
        let client = GitHubClient::new(token)?;
        let runtime = Runtime::new()?;

        Ok(Self {
            client: Arc::new(client),
            runtime,
        })
    }

    /// Helper to get a string parameter.
    fn get_str<'a>(params: &'a HashMap<String, Value>, key: &str) -> Option<&'a str> {
        params.get(key).and_then(|v| v.as_str())
    }

    /// Helper to get an i32 parameter with default.
    fn get_i32(params: &HashMap<String, Value>, key: &str, default: i32) -> i32 {
        params
            .get(key)
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(default)
    }

    /// Parse owner/repo from "owner/repo" format.
    fn parse_repo(repo_str: &str) -> Result<(&str, &str)> {
        let parts: Vec<&str> = repo_str.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid repo format. Expected 'owner/repo', got: {}",
                repo_str
            );
        }
        Ok((parts[0], parts[1]))
    }

    // ========================================================================
    // Method implementations
    // ========================================================================

    fn health(&self) -> Result<Value> {
        let client = self.client.clone();
        let ok = self.runtime.block_on(async move { client.ping().await })?;

        Ok(serde_json::json!({
            "status": if ok { "healthy" } else { "unhealthy" },
            "api_connected": ok,
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    fn get_user(&self) -> Result<Value> {
        let client = self.client.clone();
        let user = self.runtime.block_on(async move { client.get_user().await })?;

        Ok(serde_json::json!(user))
    }

    fn list_repos(&self, params: HashMap<String, Value>) -> Result<Value> {
        let limit = Self::get_i32(&params, "limit", 10);
        let client = self.client.clone();

        let repos = self
            .runtime
            .block_on(async move { client.list_repos(limit).await })?;

        Ok(serde_json::json!({
            "repos": repos,
            "count": repos.len(),
        }))
    }

    fn list_issues(&self, params: HashMap<String, Value>) -> Result<Value> {
        let repo_str = Self::get_str(&params, "repo")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: repo"))?;
        let (owner, repo) = Self::parse_repo(repo_str)?;
        let state = Self::get_str(&params, "state").unwrap_or("open");
        let limit = Self::get_i32(&params, "limit", 10);

        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();
        let state = state.to_string();
        let state_for_response = state.clone();

        let issues = self.runtime.block_on(async move {
            client.list_issues(&owner, &repo, &state, limit).await
        })?;

        Ok(serde_json::json!({
            "repo": repo_str,
            "state": state_for_response,
            "issues": issues,
            "count": issues.len(),
        }))
    }

    fn list_prs(&self, params: HashMap<String, Value>) -> Result<Value> {
        let repo_str = Self::get_str(&params, "repo")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: repo"))?;
        let (owner, repo) = Self::parse_repo(repo_str)?;
        let state = Self::get_str(&params, "state").unwrap_or("open");
        let limit = Self::get_i32(&params, "limit", 10);

        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();
        let state = state.to_string();
        let state_for_response = state.clone();

        let prs = self.runtime.block_on(async move {
            client.list_prs(&owner, &repo, &state, limit).await
        })?;

        Ok(serde_json::json!({
            "repo": repo_str,
            "state": state_for_response,
            "prs": prs,
            "count": prs.len(),
        }))
    }

    fn get_pr(&self, params: HashMap<String, Value>) -> Result<Value> {
        let repo_str = Self::get_str(&params, "repo")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: repo"))?;
        let (owner, repo) = Self::parse_repo(repo_str)?;
        let number = Self::get_i32(&params, "number", 0);
        if number == 0 {
            anyhow::bail!("Missing required parameter: number");
        }

        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();

        let pr = self.runtime.block_on(async move {
            client.get_pr(&owner, &repo, number).await
        })?;

        Ok(serde_json::json!(pr))
    }

    fn get_notifications(&self, _params: HashMap<String, Value>) -> Result<Value> {
        let client = self.client.clone();

        let notifications = self
            .runtime
            .block_on(async move { client.get_notifications().await })?;

        Ok(serde_json::json!({
            "notifications": notifications,
            "unread_count": notifications.iter().filter(|n| n.unread).count(),
        }))
    }

    fn create_issue(&self, params: HashMap<String, Value>) -> Result<Value> {
        let repo_str = Self::get_str(&params, "repo")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: repo"))?;
        let (owner, repo) = Self::parse_repo(repo_str)?;
        let title = Self::get_str(&params, "title")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: title"))?;
        let body = Self::get_str(&params, "body");

        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();
        let title = title.to_string();
        let body = body.map(|s| s.to_string());

        let issue = self.runtime.block_on(async move {
            client
                .create_issue(&owner, &repo, &title, body.as_deref())
                .await
        })?;

        Ok(serde_json::json!({
            "created": true,
            "issue": issue,
        }))
    }
}

impl FgpService for GitHubService {
    fn name(&self) -> &str {
        "github"
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
        match method {
            "health" => self.health(),
            "user" | "github.user" => self.get_user(),
            "repos" | "github.repos" => self.list_repos(params),
            "issues" | "github.issues" => self.list_issues(params),
            "prs" | "github.prs" => self.list_prs(params),
            "pr" | "github.pr" => self.get_pr(params),
            "notifications" | "github.notifications" => self.get_notifications(params),
            "create_issue" | "github.create_issue" => self.create_issue(params),
            _ => anyhow::bail!("Unknown method: {}", method),
        }
    }

    fn method_list(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo {
                name: "github.user".into(),
                description: "Get current authenticated user".into(),
                params: vec![],
            },
            MethodInfo {
                name: "github.repos".into(),
                description: "List your repositories".into(),
                params: vec![ParamInfo {
                    name: "limit".into(),
                    param_type: "integer".into(),
                    required: false,
                    default: Some(serde_json::json!(10)),
                }],
            },
            MethodInfo {
                name: "github.issues".into(),
                description: "List issues for a repository".into(),
                params: vec![
                    ParamInfo {
                        name: "repo".into(),
                        param_type: "string".into(),
                        required: true,
                        default: None,
                    },
                    ParamInfo {
                        name: "state".into(),
                        param_type: "string".into(),
                        required: false,
                        default: Some(serde_json::json!("open")),
                    },
                    ParamInfo {
                        name: "limit".into(),
                        param_type: "integer".into(),
                        required: false,
                        default: Some(serde_json::json!(10)),
                    },
                ],
            },
            MethodInfo {
                name: "github.prs".into(),
                description: "List pull requests for a repository".into(),
                params: vec![
                    ParamInfo {
                        name: "repo".into(),
                        param_type: "string".into(),
                        required: true,
                        default: None,
                    },
                    ParamInfo {
                        name: "state".into(),
                        param_type: "string".into(),
                        required: false,
                        default: Some(serde_json::json!("open")),
                    },
                    ParamInfo {
                        name: "limit".into(),
                        param_type: "integer".into(),
                        required: false,
                        default: Some(serde_json::json!(10)),
                    },
                ],
            },
            MethodInfo {
                name: "github.pr".into(),
                description: "Get pull request details with reviews and status checks".into(),
                params: vec![
                    ParamInfo {
                        name: "repo".into(),
                        param_type: "string".into(),
                        required: true,
                        default: None,
                    },
                    ParamInfo {
                        name: "number".into(),
                        param_type: "integer".into(),
                        required: true,
                        default: None,
                    },
                ],
            },
            MethodInfo {
                name: "github.notifications".into(),
                description: "Get unread notifications".into(),
                params: vec![],
            },
            MethodInfo {
                name: "github.create_issue".into(),
                description: "Create a new issue".into(),
                params: vec![
                    ParamInfo {
                        name: "repo".into(),
                        param_type: "string".into(),
                        required: true,
                        default: None,
                    },
                    ParamInfo {
                        name: "title".into(),
                        param_type: "string".into(),
                        required: true,
                        default: None,
                    },
                    ParamInfo {
                        name: "body".into(),
                        param_type: "string".into(),
                        required: false,
                        default: None,
                    },
                ],
            },
        ]
    }

    fn on_start(&self) -> Result<()> {
        tracing::info!("GitHubService starting, verifying API connection...");
        let client = self.client.clone();
        self.runtime.block_on(async move {
            match client.ping().await {
                Ok(true) => {
                    tracing::info!("GitHub API connection verified");
                    Ok(())
                }
                Ok(false) => {
                    tracing::warn!("GitHub API returned empty viewer login");
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("Failed to connect to GitHub API: {}", e);
                    Err(e)
                }
            }
        })
    }

    fn health_check(&self) -> HashMap<String, HealthStatus> {
        let mut checks = HashMap::new();

        let client = self.client.clone();
        let start = std::time::Instant::now();
        let result = self.runtime.block_on(async move { client.ping().await });

        let latency = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(true) => {
                checks.insert("github_api".into(), HealthStatus::healthy_with_latency(latency));
            }
            Ok(false) => {
                checks.insert("github_api".into(), HealthStatus::unhealthy("Empty viewer login"));
            }
            Err(e) => {
                checks.insert("github_api".into(), HealthStatus::unhealthy(e.to_string()));
            }
        }

        checks
    }
}
