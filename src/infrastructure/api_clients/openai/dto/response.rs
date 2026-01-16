use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ResponseDTO {
    output: Vec<OpenAIOutputItem>,
}

#[derive(Debug, Deserialize)]
struct OpenAIOutputItem {
    #[serde(rename = "type")]
    kind: String,
    content: Option<Vec<OpenAIContentItem>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIContentItem {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl ResponseDTO {
    pub(super) fn extract_text(&self) -> String {
        let mut out = String::new();
        for item in &self.output {
            if item.kind == "message" {
                if let Some(content) = &item.content {
                    for c in content {
                        if c.kind == "output_text" {
                            if let Some(t) = &c.text {
                                out.push_str(t);
                            }
                        }
                    }
                }
            }
        }
        out
    }
}
