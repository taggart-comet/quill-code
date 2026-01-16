mod discover_objects;
mod find_files;
mod finish;
mod patch_file;
mod read_objects;
mod shell_exec;
mod structure;

pub use discover_objects::DiscoverObjects;
pub use find_files::FindFiles;
pub use finish::Finish;
pub use patch_file::PatchFile;
pub use read_objects::ReadObjects;
pub use shell_exec::ShellExec;
pub use structure::Structure;

use crate::domain::session::Request;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid xml: {0}")]
    InvalidXml(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(String),
}

/// Wrapper for parsed XML input that tools can query
pub struct ToolInput {
    raw_xml: String,
}

impl ToolInput {
    pub fn new(xml: &str) -> Result<Self, Error> {
        // Validate XML by parsing it
        roxmltree::Document::parse(xml).map_err(|e| Error::InvalidXml(e.to_string()))?;
        Ok(Self {
            raw_xml: xml.to_string(),
        })
    }

    pub fn raw(&self) -> &str {
        &self.raw_xml
    }

    /// Get text content of an element by tag name
    pub fn get_text(&self, tag: &str) -> Option<String> {
        let doc = roxmltree::Document::parse(&self.raw_xml).ok()?;
        doc.descendants()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .map(|s| s.to_string())
    }

    /// Get text content of an element, returning error if not found
    pub fn require_text(&self, tag: &str) -> Result<String, String> {
        self.get_text(tag)
            .ok_or_else(|| format!("Missing required element: <{}>", tag))
    }

    /// Get all child elements with given tag name, returning their text content
    pub fn get_list(&self, parent_tag: &str, item_tag: &str) -> Vec<String> {
        let doc = match roxmltree::Document::parse(&self.raw_xml) {
            Ok(d) => d,
            Err(_) => return vec![],
        };

        let parent = match doc.descendants().find(|n| n.has_tag_name(parent_tag)) {
            Some(p) => p,
            None => return vec![],
        };

        parent
            .children()
            .filter(|n| n.has_tag_name(item_tag))
            .filter_map(|n| n.text().map(|s| s.to_string()))
            .collect()
    }

    /// Get integer value from element
    pub fn get_int(&self, tag: &str) -> Option<i64> {
        self.get_text(tag)?.parse().ok()
    }

    /// Get integer value, returning error if not found or invalid
    pub fn require_int(&self, tag: &str) -> Result<i64, String> {
        let text = self.require_text(tag)?;
        text.parse()
            .map_err(|_| format!("Invalid integer in <{}>: {}", tag, text))
    }

    /// Parse nested elements (like hunks)
    pub fn get_elements(&self, tag: &str) -> Vec<ToolInputElement> {
        let doc = match roxmltree::Document::parse(&self.raw_xml) {
            Ok(d) => d,
            Err(_) => return vec![],
        };

        doc.descendants()
            .filter(|n| n.has_tag_name(tag))
            .map(|n| {
                // Serialize the node back to XML string for nested parsing
                let mut xml = String::new();
                serialize_node(&n, &mut xml);
                ToolInputElement { xml }
            })
            .collect()
    }

    /// Deserialize the input XML to a struct using serde
    /// This extracts the <input> element and deserializes it
    ///
    /// Example:
    /// ```rust
    /// #[derive(Deserialize)]
    /// #[serde(rename = "input")]
    /// struct MyInput {
    ///     field: String,
    /// }
    ///
    /// let parsed: MyInput = input.deserialize()?;
    /// ```
    pub fn deserialize<T>(&self) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        // Extract the <input> element content
        let doc = roxmltree::Document::parse(&self.raw_xml)
            .map_err(|e| Error::InvalidXml(e.to_string()))?;

        let input_node = doc
            .descendants()
            .find(|n| n.has_tag_name("input"))
            .ok_or_else(|| Error::Parse("No <input> element found".into()))?;

        // Serialize the input node to XML string (with proper escaping)
        let mut input_xml = String::new();
        serialize_node(&input_node, &mut input_xml);

        // Use quick-xml to deserialize
        // Note: quick-xml expects the root element name to match the struct's serde rename
        quick_xml::de::from_str(&input_xml)
            .map_err(|e| Error::Parse(format!("Failed to deserialize XML: {}", e)))
    }
}

/// Helper to serialize a node back to XML
fn serialize_node(node: &roxmltree::Node, output: &mut String) {
    if node.is_element() {
        output.push('<');
        output.push_str(node.tag_name().name());
        output.push('>');

        for child in node.children() {
            serialize_node(&child, output);
        }

        output.push_str("</");
        output.push_str(node.tag_name().name());
        output.push('>');
    } else if node.is_text() {
        if let Some(text) = node.text() {
            output.push_str(&escape_xml(text));
        }
    }
}

/// Represents a nested element that can be queried
pub struct ToolInputElement {
    xml: String,
}

impl ToolInputElement {
    pub fn get_text(&self, tag: &str) -> Option<String> {
        let doc = roxmltree::Document::parse(&self.xml).ok()?;
        doc.descendants()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .map(|s| s.to_string())
    }

    pub fn get_int(&self, tag: &str) -> Option<i64> {
        self.get_text(tag)?.parse().ok()
    }

    pub fn get_list(&self, parent_tag: &str, item_tag: &str) -> Vec<String> {
        let doc = match roxmltree::Document::parse(&self.xml) {
            Ok(d) => d,
            Err(_) => return vec![],
        };

        let parent = match doc.descendants().find(|n| n.has_tag_name(parent_tag)) {
            Some(p) => p,
            None => return vec![],
        };

        parent
            .children()
            .filter(|n| n.has_tag_name(item_tag))
            .filter_map(|n| n.text().map(|s| s.to_string()))
            .collect()
    }
}

/// Escape special XML characters
pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub struct ToolResult {
    tool_name: String,
    input_xml: String,
    is_successful: bool,
    output_xml: String,
    error_message: String,
}

impl ToolResult {
    pub fn ok(tool_name: impl Into<String>, input: &ToolInput, output: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_xml: input.raw().to_string(),
            is_successful: true,
            output_xml: output.into(),
            error_message: String::new(),
        }
    }

    pub fn error(
        tool_name: impl Into<String>,
        input: &ToolInput,
        message: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_xml: input.raw().to_string(),
            is_successful: false,
            output_xml: String::new(),
            error_message: message.into(),
        }
    }

    pub fn output_string(&self) -> String {
        if self.is_successful {
            self.output_xml.clone()
        } else {
            format!("Error: {}", self.error_message)
        }
    }

    pub fn input_string(&self) -> String {
        self.input_xml.clone()
    }

    pub fn is_successful(&self) -> bool {
        self.is_successful
    }

    /// Generate a summary string for this tool result
    pub fn summary(&self) -> String {
        if self.is_successful {
            format!("Tool `{}` was executed successfully", self.tool_name)
        } else {
            format!("Tool `{}` failed: {}", self.tool_name, self.error_message)
        }
    }
}

/// Helper function to serialize a struct to XML string for tool outputs
///
/// Example:
/// ```rust
/// #[derive(Serialize)]
/// struct MyOutput {
///     result: String,
/// }
///
/// let output = MyOutput { result: "success".to_string() };
/// let xml = serialize_output(&output)?;
/// ToolResult::ok(tool_name, input, xml)
/// ```
pub fn serialize_output<T>(value: &T) -> Result<String, Error>
where
    T: serde::Serialize,
{
    quick_xml::se::to_string(value)
        .map_err(|e| Error::Parse(format!("Failed to serialize to XML: {}", e)))
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn work(&self, input: &ToolInput, request: &dyn Request) -> ToolResult;
    fn spec(&self) -> String;
}
