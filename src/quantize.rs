/// Quantize f32 to u8 using min-max normalization
/// 
/// # Safety
/// - `output` buffer must be at least as long as `input`
pub fn quantize_f32_to_u8(input: &[f32], output: &mut [u8]) {
    if input.is_empty() {
        return;
    }
    
    let mut min = f32::MAX;
    let mut max = f32::MIN;

    for &val in input {
        // Handle NaN and infinity
        if !val.is_finite() {
            continue;
        }
        if val < min { min = val; }
        if val > max { max = val; }
    }

    // Handle case where all values are NaN/infinity
    if min > max {
        for val in output { *val = 0; }
        return;
    }

    let range = max - min;
    if range == 0.0 {
        for val in output { *val = 0; }
        return;
    }

    let scale = 255.0 / range;
    for (i, &val) in input.iter().enumerate() {
        if i >= output.len() {
            break;
        }
        let normalized = if val.is_finite() {
            ((val - min) * scale).clamp(0.0, 255.0)
        } else {
            0.0
        };
        output[i] = normalized as u8;
    }
}

/// Quantize i16 to u8 using min-max normalization
/// 
/// # Safety
/// - `output` buffer must be at least as long as `input`
pub fn quantize_i16_to_u8(input: &[i16], output: &mut [u8]) {
    if input.is_empty() {
        return;
    }
    
    let mut min = i16::MAX;
    let mut max = i16::MIN;

    for &val in input {
        if val < min { min = val; }
        if val > max { max = val; }
    }

    // Handle case where all values are the same
    if min == max {
        for val in output { *val = 0; }
        return;
    }

    let range = (max - min) as f32;
    let scale = 255.0 / range;
    
    for (i, &val) in input.iter().enumerate() {
        if i >= output.len() {
            break;
        }
        let normalized = ((val - min) as f32 * scale).clamp(0.0, 255.0);
        output[i] = normalized as u8;
    }
}
