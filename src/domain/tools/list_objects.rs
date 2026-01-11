use crate::domain::tools::Tool;

pub struct FindObjects;

impl Tool for FindObjects {
    fn name(&self) -> &'static str {
        "find_objects"
    }

    fn work(&self, _input: &str) -> &'static str {
        ""
    }

    fn desc(&self) -> &'static str {
        "Lists objects by query (stub)"
    }

    fn format(&self) -> &'static str {
        "
input:
  file_name: string # full path to the file
  query: string  # what to search for
output:
  results: array[string]
"
    }
}
