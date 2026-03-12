#[must_use]
pub(super) fn read_non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[must_use]
pub(super) fn read_raw_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[must_use]
pub(super) fn read_env_usize(name: &str, default_value: usize, min_value: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value >= min_value)
        .unwrap_or(default_value)
}

#[must_use]
pub(super) fn read_env_usize_optional(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
}

#[must_use]
pub(super) fn read_env_u16(name: &str) -> Option<u16> {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u16>().ok())
}

#[must_use]
pub(super) fn read_env_u32(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

#[must_use]
pub(super) fn read_env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
}

#[must_use]
pub(super) fn read_env_f32(name: &str) -> Option<f32> {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<f32>().ok())
}

#[must_use]
pub(super) fn parse_enabled_default_true(raw: Option<&str>) -> bool {
    !matches!(
        raw.map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("off" | "none" | "0" | "false")
    )
}
