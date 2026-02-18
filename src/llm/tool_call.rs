use super::ToolCall;

/// Parse tool calls from assistant response text
///
/// Looks for XML-style tool calls like:
/// ```xml
/// <tool_call>
/// <name>read_file</name>
/// <arguments>{"path": "/foo/bar.txt"}</arguments>
/// </tool_call>
/// ```
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut id_counter = 0;

    let mut remaining = text;
    while let Some(start) = remaining.find("<tool_call>") {
        let after_start = &remaining[start + 11..];
        if let Some(end) = after_start.find("</tool_call>") {
            let block = &after_start[..end];

            if let Some(tool_call) = parse_single_tool_call(block, &mut id_counter) {
                calls.push(tool_call);
            }

            remaining = &after_start[end + 12..];
        } else {
            break;
        }
    }

    calls
}

fn parse_single_tool_call(block: &str, id_counter: &mut u32) -> Option<ToolCall> {
    let name = extract_tag_content(block, "name")?;
    let arguments_str = extract_tag_content(block, "arguments")?;

    let arguments: serde_json::Value = serde_json::from_str(arguments_str).ok()?;

    *id_counter += 1;
    Some(ToolCall {
        id: format!("call_{}", id_counter),
        name: name.to_string(),
        arguments,
    })
}

fn extract_tag_content<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = text.find(&open_tag)? + open_tag.len();
    let end = text.find(&close_tag)?;

    if start < end {
        Some(text[start..end].trim())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_tool_call_from_text() {
        let text = r#"
I'll read the file for you.

<tool_call>
<name>read_file</name>
<arguments>{"path": "/foo/bar.txt"}</arguments>
</tool_call>
"#;

        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments["path"], "/foo/bar.txt");
    }

    #[test]
    fn parse_multiple_tool_calls() {
        let text = r#"
<tool_call>
<name>read_file</name>
<arguments>{"path": "/a.txt"}</arguments>
</tool_call>

<tool_call>
<name>write_file</name>
<arguments>{"path": "/b.txt", "content": "hello"}</arguments>
</tool_call>
"#;

        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[1].name, "write_file");
    }

    #[test]
    fn parse_no_tool_calls() {
        let text = "Just a regular response without any tool calls.";
        let calls = parse_tool_calls(text);
        assert!(calls.is_empty());
    }
}
