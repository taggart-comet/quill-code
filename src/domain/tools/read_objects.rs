use crate::domain::tools::Tool;

pub struct ListObjects;

impl Tool for ListObjects {
    fn name(&self) -> &'static str {
        "list_objects"
    }

    fn work(&self, _input: &str) -> &'static str {
        ""
    }

    fn desc(&self) -> &'static str {
        "Lists language-aware-objects in a file (like functions, classes, etc.)"
    }

    fn format(&self) -> &'static str {
        "
input:
  file_name: string # full path to the file
output:
  results:
   classes: [string]
   functions: [string]
   constants: [string]
"
    }
}
