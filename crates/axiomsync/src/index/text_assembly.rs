use crate::models::IndexRecord;

pub(super) fn build_upsert_text(record: &IndexRecord) -> String {
    let tags_len = record.tags.iter().map(String::len).sum::<usize>();
    let tag_gap_len = record.tags.len().saturating_sub(1);
    let mut text = String::with_capacity(
        record.name.len()
            + record.abstract_text.len()
            + record.content.len()
            + tags_len
            + tag_gap_len
            + 3,
    );
    text.push_str(&record.name);
    text.push(' ');
    text.push_str(&record.abstract_text);
    text.push(' ');
    text.push_str(&record.content);
    text.push(' ');
    for (index, tag) in record.tags.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        text.push_str(tag);
    }
    text
}
