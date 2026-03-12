pub(super) const LANE_SEMANTIC: &str = "semantic";
pub(super) const LANE_EMBEDDING: &str = "embedding";

pub(super) fn lane_for_event_type(event_type: &str) -> &'static str {
    if event_type == "upsert" || event_type.starts_with("embedding_") {
        LANE_EMBEDDING
    } else {
        LANE_SEMANTIC
    }
}
