use std::path::Path;
use tree_sitter::{Language, Parser, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    Java,
    C,
    Cpp,
    Ruby,
    Json,
    Toml,
    Html,
    Css,
    Bash,
    Markdown,
}

impl Lang {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(Lang::Rust),
            "py" | "pyi" | "pyw" => Some(Lang::Python),
            "js" | "mjs" | "cjs" => Some(Lang::JavaScript),
            "ts" | "mts" | "cts" => Some(Lang::TypeScript),
            "tsx" | "jsx" => Some(Lang::Tsx),
            "go" => Some(Lang::Go),
            "java" => Some(Lang::Java),
            "c" | "h" => Some(Lang::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some(Lang::Cpp),
            "rb" | "rake" | "gemspec" => Some(Lang::Ruby),
            "json" => Some(Lang::Json),
            "toml" => Some(Lang::Toml),
            "html" | "htm" => Some(Lang::Html),
            "css" | "scss" | "sass" => Some(Lang::Css),
            "sh" | "bash" | "zsh" => Some(Lang::Bash),
            "md" | "markdown" => Some(Lang::Markdown),
            _ => None,
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Lang::Rust => "Rust",
            Lang::Python => "Python",
            Lang::JavaScript => "JavaScript",
            Lang::TypeScript => "TypeScript",
            Lang::Tsx => "TSX",
            Lang::Go => "Go",
            Lang::Java => "Java",
            Lang::C => "C",
            Lang::Cpp => "C++",
            Lang::Ruby => "Ruby",
            Lang::Json => "JSON",
            Lang::Toml => "TOML",
            Lang::Html => "HTML",
            Lang::Css => "CSS",
            Lang::Bash => "Bash",
            Lang::Markdown => "Markdown",
        }
    }

    pub fn tree_sitter_language(&self) -> Language {
        match self {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::C => tree_sitter_c::LANGUAGE.into(),
            Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Lang::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Lang::Json => tree_sitter_json::LANGUAGE.into(),
            Lang::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Lang::Html => tree_sitter_html::LANGUAGE.into(),
            Lang::Css => tree_sitter_css::LANGUAGE.into(),
            Lang::Bash => tree_sitter_bash::LANGUAGE.into(),
            Lang::Markdown => tree_sitter_md::LANGUAGE.into(),
        }
    }

    pub fn object_node_types(&self) -> &[ObjectNodeMapping] {
        match self {
            Lang::Rust => &RUST_MAPPINGS,
            Lang::Python => &PYTHON_MAPPINGS,
            Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &JS_TS_MAPPINGS,
            Lang::Go => &GO_MAPPINGS,
            Lang::Java => &JAVA_MAPPINGS,
            Lang::C => &C_MAPPINGS,
            Lang::Cpp => &CPP_MAPPINGS,
            Lang::Ruby => &RUBY_MAPPINGS,
            Lang::Json | Lang::Toml => &CONFIG_MAPPINGS,
            Lang::Html => &HTML_MAPPINGS,
            Lang::Css => &CSS_MAPPINGS,
            Lang::Bash => &BASH_MAPPINGS,
            Lang::Markdown => &MARKDOWN_MAPPINGS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Impl,
    Module,
    Constant,
    Variable,
    Type,
    Macro,
    Import,
    Export,
    Property,
    Field,
    Section,
    Rule,
}

impl ObjectKind {
    pub fn name(&self) -> &'static str {
        match self {
            ObjectKind::Function => "function",
            ObjectKind::Method => "method",
            ObjectKind::Class => "class",
            ObjectKind::Struct => "struct",
            ObjectKind::Enum => "enum",
            ObjectKind::Interface => "interface",
            ObjectKind::Trait => "trait",
            ObjectKind::Impl => "impl",
            ObjectKind::Module => "module",
            ObjectKind::Constant => "constant",
            ObjectKind::Variable => "variable",
            ObjectKind::Type => "type",
            ObjectKind::Macro => "macro",
            ObjectKind::Import => "import",
            ObjectKind::Export => "export",
            ObjectKind::Property => "property",
            ObjectKind::Field => "field",
            ObjectKind::Section => "section",
            ObjectKind::Rule => "rule",
        }
    }

}

#[derive(Debug, Clone)]
pub struct ObjectNodeMapping {
    pub node_type: &'static str,
    pub kind: ObjectKind,
    pub name_field: Option<&'static str>,
    pub name_child_type: Option<&'static str>,
}

impl ObjectNodeMapping {
    const fn new(node_type: &'static str, kind: ObjectKind) -> Self {
        Self {
            node_type,
            kind,
            name_field: Some("name"),
            name_child_type: None,
        }
    }

    const fn with_name_child(mut self, child_type: &'static str) -> Self {
        self.name_field = None;
        self.name_child_type = Some(child_type);
        self
    }

    const fn no_name(mut self) -> Self {
        self.name_field = None;
        self.name_child_type = None;
        self
    }
}

static RUST_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_item", ObjectKind::Function),
    ObjectNodeMapping::new("struct_item", ObjectKind::Struct),
    ObjectNodeMapping::new("enum_item", ObjectKind::Enum),
    ObjectNodeMapping::new("trait_item", ObjectKind::Trait),
    ObjectNodeMapping::new("impl_item", ObjectKind::Impl).no_name(),
    ObjectNodeMapping::new("mod_item", ObjectKind::Module),
    ObjectNodeMapping::new("const_item", ObjectKind::Constant),
    ObjectNodeMapping::new("static_item", ObjectKind::Variable),
    ObjectNodeMapping::new("type_item", ObjectKind::Type),
    ObjectNodeMapping::new("macro_definition", ObjectKind::Macro),
    ObjectNodeMapping::new("use_declaration", ObjectKind::Import).no_name(),
];

static PYTHON_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_definition", ObjectKind::Function),
    ObjectNodeMapping::new("class_definition", ObjectKind::Class),
    ObjectNodeMapping::new("decorated_definition", ObjectKind::Function).no_name(),
    ObjectNodeMapping::new("import_statement", ObjectKind::Import).no_name(),
    ObjectNodeMapping::new("import_from_statement", ObjectKind::Import).no_name(),
    ObjectNodeMapping::new("assignment", ObjectKind::Variable).no_name(),
];

static JS_TS_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_declaration", ObjectKind::Function),
    ObjectNodeMapping::new("function", ObjectKind::Function),
    ObjectNodeMapping::new("arrow_function", ObjectKind::Function).no_name(),
    ObjectNodeMapping::new("method_definition", ObjectKind::Method),
    ObjectNodeMapping::new("class_declaration", ObjectKind::Class),
    ObjectNodeMapping::new("class", ObjectKind::Class),
    ObjectNodeMapping::new("interface_declaration", ObjectKind::Interface),
    ObjectNodeMapping::new("type_alias_declaration", ObjectKind::Type),
    ObjectNodeMapping::new("enum_declaration", ObjectKind::Enum),
    ObjectNodeMapping::new("import_statement", ObjectKind::Import).no_name(),
    ObjectNodeMapping::new("export_statement", ObjectKind::Export).no_name(),
    ObjectNodeMapping::new("variable_declaration", ObjectKind::Variable).no_name(),
    ObjectNodeMapping::new("lexical_declaration", ObjectKind::Variable).no_name(),
];

static GO_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_declaration", ObjectKind::Function),
    ObjectNodeMapping::new("method_declaration", ObjectKind::Method),
    ObjectNodeMapping::new("type_declaration", ObjectKind::Type).no_name(),
    ObjectNodeMapping::new("type_spec", ObjectKind::Type),
    ObjectNodeMapping::new("struct_type", ObjectKind::Struct).no_name(),
    ObjectNodeMapping::new("interface_type", ObjectKind::Interface).no_name(),
    ObjectNodeMapping::new("const_declaration", ObjectKind::Constant).no_name(),
    ObjectNodeMapping::new("var_declaration", ObjectKind::Variable).no_name(),
    ObjectNodeMapping::new("import_declaration", ObjectKind::Import).no_name(),
];

static JAVA_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("method_declaration", ObjectKind::Method),
    ObjectNodeMapping::new("constructor_declaration", ObjectKind::Function),
    ObjectNodeMapping::new("class_declaration", ObjectKind::Class),
    ObjectNodeMapping::new("interface_declaration", ObjectKind::Interface),
    ObjectNodeMapping::new("enum_declaration", ObjectKind::Enum),
    ObjectNodeMapping::new("field_declaration", ObjectKind::Field).no_name(),
    ObjectNodeMapping::new("import_declaration", ObjectKind::Import).no_name(),
    ObjectNodeMapping::new("constant_declaration", ObjectKind::Constant).no_name(),
];

static C_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_definition", ObjectKind::Function)
        .with_name_child("function_declarator"),
    ObjectNodeMapping::new("declaration", ObjectKind::Variable).no_name(),
    ObjectNodeMapping::new("struct_specifier", ObjectKind::Struct).no_name(),
    ObjectNodeMapping::new("enum_specifier", ObjectKind::Enum).no_name(),
    ObjectNodeMapping::new("type_definition", ObjectKind::Type).no_name(),
    ObjectNodeMapping::new("preproc_def", ObjectKind::Macro),
    ObjectNodeMapping::new("preproc_function_def", ObjectKind::Macro),
];

static CPP_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_definition", ObjectKind::Function)
        .with_name_child("function_declarator"),
    ObjectNodeMapping::new("declaration", ObjectKind::Variable).no_name(),
    ObjectNodeMapping::new("class_specifier", ObjectKind::Class),
    ObjectNodeMapping::new("struct_specifier", ObjectKind::Struct).no_name(),
    ObjectNodeMapping::new("enum_specifier", ObjectKind::Enum).no_name(),
    ObjectNodeMapping::new("namespace_definition", ObjectKind::Module),
    ObjectNodeMapping::new("template_declaration", ObjectKind::Type).no_name(),
    ObjectNodeMapping::new("type_definition", ObjectKind::Type).no_name(),
    ObjectNodeMapping::new("preproc_def", ObjectKind::Macro),
    ObjectNodeMapping::new("preproc_function_def", ObjectKind::Macro),
];

static RUBY_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("method", ObjectKind::Method),
    ObjectNodeMapping::new("singleton_method", ObjectKind::Method),
    ObjectNodeMapping::new("class", ObjectKind::Class),
    ObjectNodeMapping::new("module", ObjectKind::Module),
    ObjectNodeMapping::new("constant", ObjectKind::Constant).no_name(),
];

static CONFIG_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("pair", ObjectKind::Property).no_name(),
    ObjectNodeMapping::new("table", ObjectKind::Section).no_name(),
    ObjectNodeMapping::new("block_mapping_pair", ObjectKind::Property).no_name(),
];

static HTML_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("element", ObjectKind::Section).no_name(),
    ObjectNodeMapping::new("script_element", ObjectKind::Section).no_name(),
    ObjectNodeMapping::new("style_element", ObjectKind::Section).no_name(),
];

static CSS_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("rule_set", ObjectKind::Rule).no_name(),
    ObjectNodeMapping::new("media_statement", ObjectKind::Rule).no_name(),
    ObjectNodeMapping::new("keyframes_statement", ObjectKind::Rule).no_name(),
];

static BASH_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("function_definition", ObjectKind::Function),
    ObjectNodeMapping::new("variable_assignment", ObjectKind::Variable).no_name(),
];

static MARKDOWN_MAPPINGS: &[ObjectNodeMapping] = &[
    ObjectNodeMapping::new("atx_heading", ObjectKind::Section).no_name(),
    ObjectNodeMapping::new("setext_heading", ObjectKind::Section).no_name(),
    ObjectNodeMapping::new("fenced_code_block", ObjectKind::Section).no_name(),
];

#[derive(Debug, Clone)]
pub struct ParsedObject {
    pub name: String,
    pub kind: ObjectKind,
    pub line_start: usize,
    pub line_end: usize,
    pub visibility: Option<String>,
}

pub struct TreeSitterParser {
    parser: Parser,
}

impl TreeSitterParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    pub fn parse(&mut self, source: &str, lang: Lang) -> Result<Vec<ParsedObject>, String> {
        self.parser
            .set_language(&lang.tree_sitter_language())
            .map_err(|e| format!("Failed to set language: {}", e))?;

        let tree = self
            .parser
            .parse(source, None)
            .ok_or("Failed to parse source code")?;

        Ok(self.extract_objects(&tree, source.as_bytes(), lang))
    }

    pub fn parse_file(&mut self, path: &str) -> Result<(Lang, Vec<ParsedObject>), String> {
        let lang =
            Lang::from_path(path).ok_or_else(|| format!("Unsupported file extension: {}", path))?;

        let source =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

        let objects = self.parse(&source, lang)?;
        Ok((lang, objects))
    }

    fn extract_objects(&self, tree: &Tree, source: &[u8], lang: Lang) -> Vec<ParsedObject> {
        let mut objects = Vec::new();
        let mappings = lang.object_node_types();

        self.visit_node(tree.root_node(), source, mappings, &mut objects, lang);

        objects
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        mappings: &[ObjectNodeMapping],
        objects: &mut Vec<ParsedObject>,
        lang: Lang,
    ) {
        let node_type = node.kind();

        if let Some(mapping) = mappings.iter().find(|m| m.node_type == node_type) {
            if let Some(obj) = self.extract_object(node, source, mapping, lang) {
                objects.push(obj);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, mappings, objects, lang);
        }
    }

    fn extract_object(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        mapping: &ObjectNodeMapping,
        lang: Lang,
    ) -> Option<ParsedObject> {
        let name = self.extract_name(node, source, mapping, lang)?;
        let visibility = self.extract_visibility(node, source, lang);

        Some(ParsedObject {
            name,
            kind: mapping.kind,
            line_start: node.start_position().row + 1,
            line_end: node.end_position().row + 1,
            visibility,
        })
    }

    fn extract_name(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        mapping: &ObjectNodeMapping,
        lang: Lang,
    ) -> Option<String> {
        if let Some(field) = mapping.name_field {
            if let Some(name_node) = node.child_by_field_name(field) {
                return name_node.utf8_text(source).ok().map(|s| s.to_string());
            }
        }

        if let Some(child_type) = mapping.name_child_type {
            return self.find_name_in_children(node, source, child_type);
        }

        match lang {
            Lang::Rust if mapping.kind == ObjectKind::Impl => {
                self.extract_rust_impl_name(node, source)
            }
            _ => self.extract_fallback_name(node, source, mapping.kind),
        }
    }

    fn find_name_in_children(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        target_type: &str,
    ) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == target_type {
                if let Some(name_node) = child.child_by_field_name("name") {
                    return name_node.utf8_text(source).ok().map(|s| s.to_string());
                }
                if let Some(ident) = child.child_by_field_name("declarator") {
                    return self.find_identifier(ident, source);
                }
                return child.utf8_text(source).ok().map(|s| s.to_string());
            }

            if let Some(name) = self.find_name_in_children(child, source, target_type) {
                return Some(name);
            }
        }
        None
    }

    fn find_identifier(&self, node: tree_sitter::Node, source: &[u8]) -> Option<String> {
        if node.kind() == "identifier" {
            return node.utf8_text(source).ok().map(|s| s.to_string());
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(name) = self.find_identifier(child, source) {
                return Some(name);
            }
        }
        None
    }

    fn extract_rust_impl_name(&self, node: tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        let mut trait_name: Option<String> = None;
        let mut type_name: Option<String> = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" | "generic_type" | "scoped_type_identifier" => {
                    let text = child.utf8_text(source).ok()?.to_string();
                    if trait_name.is_none() && type_name.is_none() {
                        type_name = Some(text);
                    } else if type_name.is_some() && trait_name.is_none() {
                        trait_name = type_name.take();
                        type_name = Some(text);
                    }
                }
                _ => {}
            }
        }

        match (&trait_name, &type_name) {
            (Some(t), Some(ty)) => Some(format!("{} for {}", t, ty)),
            (None, Some(ty)) => Some(ty.clone()),
            _ => Some("<impl>".to_string()),
        }
    }

    fn extract_fallback_name(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        kind: ObjectKind,
    ) -> Option<String> {
        let text = node.utf8_text(source).ok()?;
        let preview: String = text.chars().take(50).collect();
        let preview = preview.lines().next().unwrap_or(&preview);

        Some(format!("<{}: {}>", kind.name(), preview.trim()))
    }

    fn extract_visibility(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        lang: Lang,
    ) -> Option<String> {
        match lang {
            Lang::Rust => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "visibility_modifier" {
                        return child.utf8_text(source).ok().map(|s| s.to_string());
                    }
                }
                None
            }
            Lang::Java | Lang::TypeScript | Lang::Tsx => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "modifiers" || child.kind().contains("modifier") {
                        let text = child.utf8_text(source).ok()?;
                        if text.contains("public")
                            || text.contains("private")
                            || text.contains("protected")
                        {
                            return Some(text.to_string());
                        }
                    }
                }
                None
            }
            Lang::Python => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node.utf8_text(source).ok()?;
                    if name.starts_with("__") && !name.ends_with("__") {
                        return Some("private".to_string());
                    } else if name.starts_with("_") {
                        return Some("protected".to_string());
                    }
                }
                None
            }
            _ => None,
        }
    }
}

impl Default for TreeSitterParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_detection() {
        assert_eq!(Lang::from_extension("rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_extension("py"), Some(Lang::Python));
        assert_eq!(Lang::from_extension("js"), Some(Lang::JavaScript));
        assert_eq!(Lang::from_extension("ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("go"), Some(Lang::Go));
        assert_eq!(Lang::from_extension("java"), Some(Lang::Java));
        assert_eq!(Lang::from_extension("php"), None);
        assert_eq!(Lang::from_extension("kt"), None);
    }

    #[test]
    fn test_parse_rust() {
        let source = r#"
pub fn hello() {}
struct Point { x: i32, y: i32 }
impl Point {
    fn new() -> Self { Self { x: 0, y: 0 } }
}
"#;
        let mut parser = TreeSitterParser::new();
        let objects = parser.parse(source, Lang::Rust).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "Point"));
        assert!(objects.iter().any(|o| o.name == "new"));
    }

    #[test]
    fn test_parse_python() {
        let source = r#"
def hello():
    pass

class MyClass:
    def method(self):
        pass
"#;
        let mut parser = TreeSitterParser::new();
        let objects = parser.parse(source, Lang::Python).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "MyClass"));
        assert!(objects.iter().any(|o| o.name == "method"));
    }

    #[test]
    fn test_parse_javascript() {
        let source = r#"
function hello() {}
class MyClass {
    method() {}
}
const x = 1;
"#;
        let mut parser = TreeSitterParser::new();
        let objects = parser.parse(source, Lang::JavaScript).unwrap();

        assert!(objects.iter().any(|o| o.name == "hello"));
        assert!(objects.iter().any(|o| o.name == "MyClass"));
    }
}
