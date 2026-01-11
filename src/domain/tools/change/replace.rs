use crate::domain::tools::Tool;

pub struct Insert;

impl Tool for Insert {
    fn name(&self) -> &'static str {
        "change_insert"
    }

    fn work(&self, _input: &str) -> String {
        String::new()
    }

    fn desc(&self) -> &'static str {
        "Apply an edit to an existing file"
    }

    fn format(&self) -> &'static str {
        "
input:
  file_name: string  # full path to the file
  target_line: string  # line in the file where to insert the change
  insert_content: string  # content to insert
output:
  ok: boolean
  error: string  # optional
"
    }
}
