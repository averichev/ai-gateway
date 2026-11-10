use std::collections::HashSet;

use serde_json::Value;

use super::{
    GenerateMessageDto, GenerateRequestDto, ToolCallDto, ToolChoiceDto, ToolChoicePolicy,
    ToolDefinitionDto,
};

pub const MAX_GENERATE_BODY_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TOOLS: usize = 128;
const MAX_TOOL_JSON_BYTES: usize = 64 * 1024;

pub fn valid_tool_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-')
}

pub fn valid_tool_call(call: &ToolCallDto) -> bool {
    (1..=256).contains(&call.id.len())
        && call.id.bytes().all(|ch| ch.is_ascii_graphic())
        && valid_tool_name(&call.name)
        && serde_json::to_vec(&call.arguments).is_ok_and(|json| json.len() <= MAX_TOOL_JSON_BYTES)
}

impl ToolDefinitionDto {
    /// Network and file resolution are disabled in the jsonschema dependency.
    /// Never include validation errors: they may contain user schema values.
    pub fn validator(&self) -> Result<jsonschema::Validator, &'static str> {
        let schema = Value::Object(self.parameters.clone());
        if schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err("Корень parameters должен иметь type: object");
        }
        if serde_json::to_vec(&schema).map_or(true, |json| json.len() > MAX_TOOL_JSON_BYTES) {
            return Err("JSON Schema инструмента превышает 64 KiB");
        }
        jsonschema::options()
            .build(&schema)
            .map_err(|_| "Некорректная JSON Schema или недоступная внешняя ссылка")
    }
}

impl GenerateRequestDto {
    pub fn uses_tools(&self) -> bool {
        !self.tools.is_empty()
            || self.tool_choice.is_some()
            || self.messages.iter().any(|message| match message {
                GenerateMessageDto::Tool { .. } => true,
                GenerateMessageDto::Assistant { tool_calls, .. } => !tool_calls.is_empty(),
                _ => false,
            })
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.model.trim().is_empty() || self.messages.is_empty() {
            return Err("Поля `model` и `messages` обязательны");
        }
        if self.messages.len() > 256 || self.tools.len() > MAX_TOOLS {
            return Err("Допускается не более 256 сообщений и 128 инструментов");
        }
        if serde_json::to_vec(self).map_or(true, |json| json.len() > MAX_GENERATE_BODY_BYTES) {
            return Err("Запрос превышает 2 MiB");
        }
        let mut names = HashSet::new();
        for tool in &self.tools {
            if !valid_tool_name(&tool.name) || !names.insert(tool.name.as_str()) {
                return Err(
                    "Имена инструментов должны быть уникальными: 1..64 символа [a-zA-Z0-9_-]",
                );
            }
            tool.validator()?;
        }
        match &self.tool_choice {
            Some(ToolChoiceDto::Named(tool)) if !names.contains(tool.name.as_str()) => {
                return Err("tool_choice должен ссылаться на объявленный инструмент");
            }
            Some(ToolChoiceDto::Policy(ToolChoicePolicy::Auto | ToolChoicePolicy::Required))
                if self.tools.is_empty() =>
            {
                return Err("Для tool_choice auto/required нужны tools");
            }
            _ => {}
        }

        let mut pending = HashSet::new();
        let mut seen = HashSet::new();
        for message in &self.messages {
            if let GenerateMessageDto::Tool { tool_call_id, .. } = message {
                if !pending.remove(tool_call_id.as_str()) {
                    return Err(
                        "Результат инструмента должен соответствовать ожидающему tool_call_id",
                    );
                }
                continue;
            }
            if !pending.is_empty() {
                return Err("Перед следующим сообщением нужны результаты всех tool_calls");
            }
            if let GenerateMessageDto::Assistant {
                content,
                tool_calls,
            } = message
            {
                if content.is_none() && tool_calls.is_empty() {
                    return Err("Assistant message требует content или tool_calls");
                }
                if tool_calls.len() > MAX_TOOLS {
                    return Err("Допускается не более 128 tool_calls в сообщении");
                }
                for call in tool_calls {
                    if !valid_tool_call(call) || !seen.insert(call.id.as_str()) {
                        return Err("Некорректный tool_call или повторяющийся id");
                    }
                    pending.insert(call.id.as_str());
                }
            }
        }
        if !pending.is_empty() {
            return Err("Перед генерацией нужны результаты всех tool_calls");
        }
        Ok(())
    }
}
