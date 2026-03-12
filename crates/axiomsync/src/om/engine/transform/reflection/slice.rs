use super::super::types::BufferedReflectionSlicePlan;

pub fn plan_buffered_reflection_slice(
    full_observations: &str,
    observation_token_count: u32,
    reflection_threshold: u32,
    buffer_activation: f32,
) -> BufferedReflectionSlicePlan {
    let all_lines = full_observations.lines().collect::<Vec<_>>();
    let total_lines = all_lines.len();
    let avg_tokens_per_line = if total_lines == 0 {
        0.0_f64
    } else {
        f64::from(observation_token_count) / total_lines as f64
    };

    let activation_point_tokens = f64::from(reflection_threshold) * f64::from(buffer_activation);
    let lines_to_reflect = if avg_tokens_per_line > 0.0 {
        ((activation_point_tokens / avg_tokens_per_line).floor() as usize).min(total_lines)
    } else {
        total_lines
    };
    let sliced_observations = all_lines[..lines_to_reflect].join("\n");
    let slice_token_estimate = (avg_tokens_per_line * lines_to_reflect as f64)
        .round()
        .clamp(0.0, f64::from(u32::MAX)) as u32;
    let compression_target_tokens = (f64::from(slice_token_estimate) * f64::from(buffer_activation))
        .min(f64::from(reflection_threshold))
        .ceil()
        .clamp(0.0, f64::from(u32::MAX)) as u32;

    BufferedReflectionSlicePlan {
        sliced_observations,
        slice_token_estimate,
        compression_target_tokens,
    }
}
