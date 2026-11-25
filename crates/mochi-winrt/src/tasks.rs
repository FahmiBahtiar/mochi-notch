use chrono::NaiveDateTime;
use serde_json::Value;

use crate::calendar::parse_iso_naive;
use crate::google::GoogleAuth;
use crate::http::url_encode;

#[derive(Debug, Clone)]
pub struct TaskListInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RemoteTask {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub due_date: Option<NaiveDateTime>,
}

pub fn fetch_google_task_lists(auth: &mut GoogleAuth) -> Result<Vec<TaskListInfo>, String> {
    let resp = auth.send_api(
        "GET",
        "https://tasks.googleapis.com/tasks/v1/users/@me/lists?maxResults=100&fields=items(id,title)",
        None,
    )?;
    if !(200..300).contains(&resp.status) {
        return Err(format!("HTTP {}", resp.status));
    }
    let doc: Value = serde_json::from_str(&resp.body).map_err(|e| e.to_string())?;
    let mut lists = Vec::new();
    if let Some(items) = doc.get("items").and_then(Value::as_array) {
        for item in items {
            if let (Some(id), Some(title)) = (
                item.get("id").and_then(Value::as_str),
                item.get("title").and_then(Value::as_str),
            ) {
                lists.push(TaskListInfo {
                    id: id.to_string(),
                    name: title.to_string(),
                });
            }
        }
    }
    Ok(lists)
}

pub fn fetch_google_tasks(auth: &mut GoogleAuth, list_id: &str) -> Result<Vec<RemoteTask>, String> {
    let url = format!(
        "https://tasks.googleapis.com/tasks/v1/lists/{}/tasks?showCompleted=true&showHidden=true&maxResults=100",
        url_encode(list_id)
    );
    let resp = auth.send_api("GET", &url, None)?;
    if !(200..300).contains(&resp.status) {
        return Err(format!("HTTP {}", resp.status));
    }
    let doc: Value = serde_json::from_str(&resp.body).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(items) = doc.get("items").and_then(Value::as_array) {
        for item in items {
            if item.get("deleted").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let done = item.get("status").and_then(Value::as_str) == Some("completed");
            let due_date = item
                .get("due")
                .and_then(Value::as_str)
                .and_then(parse_iso_naive);
            out.push(RemoteTask {
                id: id.to_string(),
                title,
                done,
                due_date,
            });
        }
    }
    Ok(out)
}
