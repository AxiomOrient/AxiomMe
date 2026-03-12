use crate::models::IndexRecord;

pub fn infer_mime(record: &IndexRecord) -> Option<&'static str> {
    if !record.is_leaf {
        return None;
    }

    infer_mime_from_name(&record.name)
}

pub fn infer_mime_from_name(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("text/markdown"),
        "txt" | "text" | "log" => Some("text/plain"),
        "json" => Some("application/json"),
        "jsonl" => Some("application/x-ndjson"),
        "yaml" | "yml" => Some("application/yaml"),
        "toml" => Some("application/toml"),
        "xml" => Some("application/xml"),
        "rs" => Some("text/rust"),
        "py" => Some("text/x-python"),
        "js" | "mjs" | "cjs" => Some("text/javascript"),
        "ts" | "tsx" => Some("text/typescript"),
        "jsx" => Some("text/jsx"),
        "java" => Some("text/x-java-source"),
        "go" => Some("text/x-go"),
        "c" | "h" => Some("text/x-c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("text/x-c++"),
        "sh" => Some("text/x-shellscript"),
        "ini" | "cfg" | "conf" | "env" => Some("text/plain"),
        _ => None,
    }
}
