pub fn get_tool_result(
    _model_type: crate::domain::ModelType,
    chain_step: crate::domain::workflow::ChainStep,
) -> String {
    let tool_name = chain_step.tool_name.unwrap_or_else(|| "unspecified".to_string());
    let output = chain_step.tool_output.unwrap_or_default();
    format!(
        "Tool `{}` execution output is: \n{}\n\
---\n\
Tool input was: \n\
{}\n",
        tool_name, output, chain_step.input_payload
    )
}
