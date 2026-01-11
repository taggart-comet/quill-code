use tools::Tool;

pub struct Insert;

impl Tool for Insert {
    fn name(&self) -> &'static str {
        "change"
    }

    fn work(&self, _input: &str) -> &'static str {
        ""
    }

    fn desc(&self) -> &'static str {
        "Apply an edit to an existing file"
    }

    fn format(&self) -> &'static str {
        "
input:
  target_line: string  # line in the file where to insert the change
  insert_content: string  # content to insert
output:
  ok: boolean
  error: string  # optional
"
    }
}
