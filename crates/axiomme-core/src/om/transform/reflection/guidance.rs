#[must_use]
pub fn reflector_compression_guidance(level: u8) -> &'static str {
    match level.min(2) {
        1 => {
            "## COMPRESSION REQUIRED

Your previous reflection was the same size or larger than the original observations.

Please re-process with slightly more compression:
- Towards the beginning, condense more observations into higher-level reflections
- Closer to the end, retain more fine details (recent context matters more)
- Memory is getting long - use a more condensed style throughout
- Combine related items more aggressively but do not lose important specific details of names, places, events, and people
- For example if there is a long nested observation list about repeated tool calls, you can combine those into a single line and observe that the tool was called multiple times for x reason, and finally y outcome happened.

Your current detail level was a 10/10, lets aim for a 8/10 detail level."
        }
        2 => {
            "## AGGRESSIVE COMPRESSION REQUIRED

Your previous reflection was still too large after compression guidance.

Please re-process with much more aggressive compression:
- Towards the beginning, heavily condense observations into high-level summaries
- Closer to the end, retain fine details (recent context matters more)
- Memory is getting very long - use a significantly more condensed style throughout
- Combine related items aggressively but do not lose important specific details of names, places, events, and people
- For example if there is a long nested observation list about repeated tool calls, you can combine those into a single line and observe that the tool was called multiple times for x reason, and finally y outcome happened.
- Remove redundant information and merge overlapping observations

Your current detail level was a 10/10, lets aim for a 6/10 detail level."
        }
        _ => "",
    }
}

#[must_use]
pub const fn validate_reflection_compression(
    reflected_tokens: u32,
    target_threshold_tokens: u32,
) -> bool {
    reflected_tokens < target_threshold_tokens
}
