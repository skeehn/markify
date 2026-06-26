//! LLM-powered extraction-rule generation for the parse.bot-style structured API.
//!
//! Given a page's HTML and a natural-language description, an LLM produces precise
//! CSS-selector extraction rules. The LLM runs ONCE here (at `/v1/generate`); the
//! resulting spec is then executed deterministically (no further LLM calls), so
//! repeated extraction is fast and free.

use serde::Deserialize;
use tracing::debug;

use super::spec::{Endpoint, ExtractionRule, FieldType, OutputType};

/// Resolved LLM configuration from the environment.
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl LlmConfig {
    /// Build from env vars; `None` if no API key is configured (callers then
    /// fall back to heuristic generation). Keys are tried in order:
    /// `MARKIFY_LLM_API_KEY`, `OPENROUTER_API_KEY`, `OPENAI_API_KEY`.
    /// `MARKIFY_LLM_BASE_URL` / `MARKIFY_LLM_MODEL` override the defaults.
    pub fn from_env() -> Option<Self> {
        let model = std::env::var("MARKIFY_LLM_MODEL").ok();
        if let Some(key) = non_empty_env("MARKIFY_LLM_API_KEY") {
            return Some(Self {
                base_url: std::env::var("MARKIFY_LLM_BASE_URL")
                    .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string()),
                api_key: key,
                model: model.unwrap_or_else(|| "openai/gpt-4o-mini".to_string()),
            });
        }
        if let Some(key) = non_empty_env("OPENROUTER_API_KEY") {
            return Some(Self {
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: key,
                model: model.unwrap_or_else(|| "openai/gpt-4o-mini".to_string()),
            });
        }
        if let Some(key) = non_empty_env("OPENAI_API_KEY") {
            return Some(Self {
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: key,
                model: model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
            });
        }
        None
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}
#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}
#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

/// The JSON shape we ask the LLM to return.
#[derive(Deserialize)]
struct LlmSpec {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    returns_list: bool,
    #[serde(default)]
    container_selector: Option<String>,
    fields: Vec<LlmField>,
}

#[derive(Deserialize)]
struct LlmField {
    field: String,
    selector: String,
    #[serde(default = "default_extract")]
    extract: String,
    #[serde(rename = "type", default = "default_type")]
    field_type: String,
}

fn default_extract() -> String {
    "text".to_string()
}
fn default_type() -> String {
    "string".to_string()
}

/// Ask the LLM to produce extraction rules for `description` against `html`.
pub async fn generate_endpoint_via_llm(
    html: &str,
    description: &str,
    config: &LlmConfig,
) -> anyhow::Result<Endpoint> {
    let sample = compact_html(html, 14_000);

    let system = "You convert a web page into precise CSS-selector extraction rules. \
You always respond with a single JSON object and no prose.";
    let user = format!(
        "The user wants to extract: \"{description}\"\n\n\
Here is the page HTML (may be truncated):\n```html\n{sample}\n```\n\n\
Respond with ONLY this JSON object:\n\
{{\n\
  \"name\": \"short_snake_case_name\",\n\
  \"returns_list\": true or false,\n\
  \"container_selector\": \"a CSS selector matching each repeating item, or null\",\n\
  \"fields\": [\n\
    {{\"field\": \"output_name\", \"selector\": \"CSS selector (relative to the container item when returns_list is true)\", \"extract\": \"text | href | src | attr:NAME\", \"type\": \"string | number | url | date\"}}\n\
  ]\n\
}}\n\
Rules: for a list of repeating items set returns_list=true, give container_selector for the repeating element, and make every field selector RELATIVE to that container. For a single record set returns_list=false and container_selector=null with page-absolute field selectors. Use specific, stable selectors that actually appear in the HTML. For links use extract \"href\". Output JSON only."
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/chat/completions",
            config.base_url.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": config.model,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("LLM API error: {} - {}", status, body);
    }

    let chat: ChatResponse = resp.json().await?;
    let content = chat
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| anyhow::anyhow!("LLM returned no choices"))?;

    let json_str = extract_json(&content);
    let llm_spec: LlmSpec = serde_json::from_str(&json_str)
        .map_err(|e| anyhow::anyhow!("LLM returned invalid JSON: {e}"))?;

    let rules: Vec<ExtractionRule> = llm_spec
        .fields
        .into_iter()
        .filter(|f| !f.field.is_empty() && !f.selector.is_empty())
        .map(|f| ExtractionRule {
            field: f.field,
            selector: f.selector,
            extract: f.extract,
            required: false,
            field_type: parse_field_type(&f.field_type),
        })
        .collect();

    if rules.is_empty() {
        anyhow::bail!("LLM produced no usable extraction rules");
    }

    let name = llm_spec
        .name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "extract".to_string());

    debug!(
        rules = rules.len(),
        returns_list = llm_spec.returns_list,
        "LLM generated extraction rules"
    );

    Ok(Endpoint {
        name,
        description: description.to_string(),
        container_selector: llm_spec.container_selector.filter(|s| !s.is_empty()),
        extraction_rules: rules,
        output_type: if llm_spec.returns_list {
            OutputType::List
        } else {
            OutputType::Object
        },
        returns_list: llm_spec.returns_list,
    })
}

/// Strip `<script>`/`<style>` blocks and take the first `max` chars so the model
/// sees page structure (tags + class names) rather than JS/CSS blobs.
fn compact_html(html: &str, max: usize) -> String {
    let no_script = strip_blocks(html, "<script", "</script>");
    let no_style = strip_blocks(&no_script, "<style", "</style>");
    no_style.chars().take(max).collect()
}

fn strip_blocks(input: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find(close) {
            Some(end) => &rest[start + end + close.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Pull the JSON object out of a possibly fenced / prose-wrapped LLM reply.
fn extract_json(content: &str) -> String {
    let s = content.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if end > start {
            return s[start..=end].to_string();
        }
    }
    s.trim().to_string()
}

fn parse_field_type(t: &str) -> FieldType {
    match t.to_lowercase().as_str() {
        "number" | "int" | "integer" | "float" => FieldType::Number,
        "boolean" | "bool" => FieldType::Boolean,
        "url" | "link" => FieldType::Url,
        "date" | "datetime" => FieldType::Date,
        "list" | "array" => FieldType::List,
        _ => FieldType::String,
    }
}
