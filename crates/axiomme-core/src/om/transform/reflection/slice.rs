use super::super::types::BufferedReflectionSlicePlan;

#[must_use]
pub fn plan_buffered_reflection_slice(
    full_observations: &str,
    observation_token_count: u32,
    reflection_threshold: u32,
    buffer_activation: f32,
) -> BufferedReflectionSlicePlan {
    let all_lines = full_observations.split('\n').collect::<Vec<_>>();
    let total_lines = all_lines.len();
    let total_lines_u32 = saturating_usize_to_u32(total_lines);
    let avg_tokens_per_line = if total_lines_u32 == 0 {
        0.0_f64
    } else {
        f64::from(observation_token_count) / f64::from(total_lines_u32)
    };

    let activation_point_tokens = f64::from(reflection_threshold) * f64::from(buffer_activation);
    let lines_to_reflect_u32 = if avg_tokens_per_line > 0.0 {
        floor_f64_to_u32_clamped(activation_point_tokens / avg_tokens_per_line).min(total_lines_u32)
    } else {
        total_lines_u32
    };
    let lines_to_reflect = usize::try_from(lines_to_reflect_u32)
        .unwrap_or(usize::MAX)
        .min(total_lines);
    let sliced_observations = all_lines[..lines_to_reflect].join("\n");
    let reflected_observation_line_count = lines_to_reflect_u32;
    let slice_token_estimate =
        round_f64_to_u32_clamped(avg_tokens_per_line * f64::from(lines_to_reflect_u32));
    let compression_target_tokens = ceil_f64_to_u32_clamped(
        (f64::from(slice_token_estimate) * f64::from(buffer_activation))
            .min(f64::from(reflection_threshold)),
    );

    BufferedReflectionSlicePlan {
        sliced_observations,
        reflected_observation_line_count,
        slice_token_estimate,
        compression_target_tokens,
    }
}

fn saturating_usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn floor_f64_to_u32_clamped(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let value = value.floor().min(f64::from(u32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is non-negative and bounded to u32::MAX before cast"
    )]
    {
        value as u32
    }
}

fn round_f64_to_u32_clamped(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let value = value.round().min(f64::from(u32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is non-negative and bounded to u32::MAX before cast"
    )]
    {
        value as u32
    }
}

fn ceil_f64_to_u32_clamped(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let value = value.ceil().min(f64::from(u32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is non-negative and bounded to u32::MAX before cast"
    )]
    {
        value as u32
    }
}
