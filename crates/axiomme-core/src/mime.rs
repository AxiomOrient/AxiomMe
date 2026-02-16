use crate::models::IndexRecord;

pub fn infer_mime(record: &IndexRecord) -> Option<&'static str> {
    if !record.is_leaf {
        return None;
    }

    let ext = record.name.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("text/markdown"),
        "txt" | "log" => Some("text/plain"),
        "json" => Some("application/json"),
        "rs" => Some("text/rust"),
        _ => None,
    }
}
