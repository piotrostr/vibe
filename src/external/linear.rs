use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct LinearIssue {
    pub identifier: String, // Human-readable ID like "VIB-6"
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LinearIssueStatus {
    pub identifier: String,
    pub state_type: String, // "backlog", "unstarted", "started", "completed", "cancelled"
    #[allow(dead_code)] // Kept for debug output and future use
    pub state_name: String, // Human-readable like "In Progress"
}

#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ViewerData {
    viewer: Viewer,
}

#[derive(Debug, Deserialize)]
struct Viewer {
    #[serde(rename = "assignedIssues")]
    assigned_issues: Option<IssueConnection>,
}

#[derive(Debug, Deserialize)]
struct IssueConnection {
    nodes: Vec<IssueNode>,
}

#[derive(Debug, Deserialize)]
struct IssueNode {
    identifier: String,
    title: String,
    description: Option<String>,
    url: String,
    labels: Option<LabelConnection>,
}

#[derive(Debug, Deserialize)]
struct LabelConnection {
    nodes: Vec<LabelNode>,
}

#[derive(Debug, Deserialize)]
struct LabelNode {
    name: String,
}

/// Result of creating an issue
#[derive(Debug, Clone)]
pub struct CreatedIssue {
    pub identifier: String,
    pub url: String,
}

pub struct LinearClient {
    http: Client,
    api_key: String,
}

impl LinearClient {
    const API_URL: &'static str = "https://api.linear.app/graphql";

    pub fn new(api_key: String) -> Self {
        Self {
            http: Client::new(),
            api_key,
        }
    }

    /// Get the current user's ID
    async fn get_viewer_id(&self) -> Result<String, String> {
        let query = r#"query { viewer { id } }"#;
        let body = serde_json::json!({ "query": query });

        let response = self
            .http
            .post(Self::API_URL)
            .header("Authorization", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        json.get("data")
            .and_then(|d| d.get("viewer"))
            .and_then(|v| v.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Failed to get viewer ID".to_string())
    }

    /// Get the user's default team ID (first team they belong to)
    async fn get_default_team_id(&self) -> Result<String, String> {
        let query = r#"query { teams { nodes { id name } } }"#;
        let body = serde_json::json!({ "query": query });

        let response = self
            .http
            .post(Self::API_URL)
            .header("Authorization", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        json.get("data")
            .and_then(|d| d.get("teams"))
            .and_then(|t| t.get("nodes"))
            .and_then(|n| n.as_array())
            .and_then(|arr| arr.first())
            .and_then(|team| team.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No teams found".to_string())
    }

    /// Create a new issue assigned to self in the backlog
    pub async fn create_issue(
        &self,
        title: &str,
        description: Option<&str>,
    ) -> Result<CreatedIssue, String> {
        let viewer_id = self.get_viewer_id().await?;
        let team_id = self.get_default_team_id().await?;

        let desc_value = description
            .map(|d| format!(r#""{}""#, d.replace('"', "\\\"")))
            .unwrap_or_else(|| "null".to_string());

        let query = format!(
            r#"mutation {{
                issueCreate(input: {{
                    title: "{}",
                    description: {},
                    teamId: "{}",
                    assigneeId: "{}"
                }}) {{
                    success
                    issue {{
                        identifier
                        url
                    }}
                }}
            }}"#,
            title.replace('"', "\\\""),
            desc_value,
            team_id,
            viewer_id
        );

        let body = serde_json::json!({ "query": query });

        let response = self
            .http
            .post(Self::API_URL)
            .header("Authorization", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if let Some(errors) = json.get("errors") {
            return Err(format!("GraphQL error: {}", errors));
        }

        let issue = json
            .get("data")
            .and_then(|d| d.get("issueCreate"))
            .and_then(|ic| ic.get("issue"))
            .ok_or("Failed to create issue")?;

        let identifier = issue
            .get("identifier")
            .and_then(|i| i.as_str())
            .ok_or("Missing identifier")?
            .to_string();

        let url = issue
            .get("url")
            .and_then(|u| u.as_str())
            .ok_or("Missing url")?
            .to_string();

        Ok(CreatedIssue { identifier, url })
    }

    /// Fetch backlog issues assigned to the current user (API key owner)
    pub async fn fetch_backlog_issues(&self) -> Result<Vec<LinearIssue>, String> {
        let query = r#"
            query {
                viewer {
                    assignedIssues(filter: { state: { type: { eq: "backlog" } } }) {
                        nodes {
                            identifier
                            title
                            description
                            url
                            labels {
                                nodes {
                                    name
                                }
                            }
                        }
                    }
                }
            }
        "#;

        let body = serde_json::json!({ "query": query });

        let response = self
            .http
            .post(Self::API_URL)
            .header("Authorization", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                text.chars().take(200).collect::<String>()
            ));
        }

        let result: GraphQLResponse<ViewerData> = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if let Some(errors) = result.errors {
            let msg = errors
                .iter()
                .map(|e| e.message.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("GraphQL error: {}", msg));
        }

        let data = result.data.ok_or("No data in response")?;
        let issues = data
            .viewer
            .assigned_issues
            .map(|c| c.nodes)
            .unwrap_or_default();

        Ok(issues
            .into_iter()
            .map(|node| LinearIssue {
                identifier: node.identifier,
                title: node.title,
                description: node.description,
                url: node.url,
                labels: node
                    .labels
                    .map(|l| l.nodes.into_iter().map(|n| n.name).collect())
                    .unwrap_or_default(),
            })
            .collect())
    }

    /// Fetch status for multiple issues by identifiers
    /// Uses GraphQL aliases to batch multiple `issue` queries into one request
    pub async fn fetch_issue_statuses(
        &self,
        identifiers: &[String],
    ) -> Result<Vec<LinearIssueStatus>, String> {
        if identifiers.is_empty() {
            return Ok(Vec::new());
        }

        // Build a query with aliases for each identifier
        // e.g., query { i0: issue(id: "VIB-5") { ... } i1: issue(id: "VIB-6") { ... } }
        let fields: Vec<String> = identifiers
            .iter()
            .enumerate()
            .map(|(i, id)| {
                format!(
                    r#"i{}: issue(id: "{}") {{ identifier state {{ name type }} }}"#,
                    i, id
                )
            })
            .collect();

        let query = format!("query {{ {} }}", fields.join(" "));

        let body = serde_json::json!({ "query": query });

        let response = self
            .http
            .post(Self::API_URL)
            .header("Authorization", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                text.chars().take(200).collect::<String>()
            ));
        }

        // Parse as dynamic JSON since the response shape depends on aliases
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if let Some(errors) = json.get("errors")
            && let Some(arr) = errors.as_array()
        {
            let msgs: Vec<String> = arr
                .iter()
                .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                .map(|s| s.to_string())
                .collect();
            if !msgs.is_empty() {
                return Err(format!("GraphQL error: {}", msgs.join(", ")));
            }
        }

        let data = json.get("data").ok_or("No data in response")?;

        let mut statuses = Vec::new();
        for i in 0..identifiers.len() {
            let key = format!("i{}", i);
            if let Some(issue) = data.get(&key) {
                // issue can be null if not found
                if issue.is_null() {
                    continue;
                }
                let identifier = issue
                    .get("identifier")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let state = issue.get("state").ok_or("Missing state field")?;
                let state_name = state
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let state_type = state
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                statuses.push(LinearIssueStatus {
                    identifier,
                    state_type,
                    state_name,
                });
            }
        }

        Ok(statuses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_api_key() -> Option<String> {
        std::env::var("VIBE_KANBAN_LINEAR_API_KEY").ok()
    }

    #[tokio::test]
    async fn test_fetch_backlog_issues() {
        let Some(api_key) = get_test_api_key() else {
            eprintln!("Skipping test: VIBE_KANBAN_LINEAR_API_KEY not set");
            return;
        };

        let client = LinearClient::new(api_key);
        let result = client.fetch_backlog_issues().await;

        match result {
            Ok(issues) => {
                println!("Found {} assigned backlog issues", issues.len());
                for issue in &issues {
                    println!("  - {} [{}]", issue.title, issue.identifier);
                }
            }
            Err(e) => {
                panic!("Failed to fetch issues: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_issue_statuses() {
        let Some(api_key) = get_test_api_key() else {
            eprintln!("Skipping test: VIBE_KANBAN_LINEAR_API_KEY not set");
            return;
        };

        let client = LinearClient::new(api_key);
        // Use a non-existent ID to test the empty case, and the real API response
        let result = client.fetch_issue_statuses(&["VIB-999".to_string()]).await;

        match result {
            Ok(statuses) => {
                println!("Fetched {} issue statuses", statuses.len());
                for status in &statuses {
                    println!(
                        "  - {} [{}]: {}",
                        status.identifier, status.state_type, status.state_name
                    );
                }
            }
            Err(e) => {
                panic!("Failed to fetch issue statuses: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_issue_statuses_empty() {
        let Some(api_key) = get_test_api_key() else {
            eprintln!("Skipping test: VIBE_KANBAN_LINEAR_API_KEY not set");
            return;
        };

        let client = LinearClient::new(api_key);
        let result = client.fetch_issue_statuses(&[]).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
