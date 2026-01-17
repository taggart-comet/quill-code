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
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIContentItem {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl ResponseDTO {
    pub(crate) fn extract_parts(&self) -> (String, Option<FunctionCall>) {
        let mut summary = String::new();
        let mut tool_call: Option<FunctionCall> = None;

        for item in &self.output {
            if item.kind == "message" {
                if let Some(content) = &item.content {
                    for c in content {
                        match c.kind.as_str() {
                            "output_text" => {
                                if let Some(t) = &c.text {
                                    summary.push_str(t);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            } else if item.kind == "function_call" {
                if let (Some(name), Some(arguments)) =
                    (item.name.as_ref(), item.arguments.as_ref())
                {
                    tool_call = Some(FunctionCall {
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    });
                }
            }
        }

        (summary, tool_call)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FunctionCall {
    pub name: String,
    pub arguments: String,
}
