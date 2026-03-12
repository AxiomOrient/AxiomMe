pub(super) fn parse_unit_interval_f32(raw: &str) -> std::result::Result<f32, String> {
    let value = raw
        .parse::<f32>()
        .map_err(|_| format!("invalid float value '{raw}'"))?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("value must be within [0.0, 1.0], got {value}"));
    }
    Ok(value)
}

pub(super) fn parse_non_negative_f32(raw: &str) -> std::result::Result<f32, String> {
    let value = raw
        .parse::<f32>()
        .map_err(|_| format!("invalid float value '{raw}'"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("value must be finite and >= 0, got {value}"));
    }
    Ok(value)
}

pub(super) fn parse_min_one_u32(raw: &str) -> std::result::Result<u32, String> {
    let value = raw
        .parse::<u32>()
        .map_err(|_| format!("invalid integer value '{raw}'"))?;
    if value == 0 {
        return Err("value must be >= 1".to_string());
    }
    Ok(value)
}

pub(super) fn parse_min_one_usize(raw: &str) -> std::result::Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("invalid integer value '{raw}'"))?;
    if value == 0 {
        return Err("value must be >= 1".to_string());
    }
    Ok(value)
}
