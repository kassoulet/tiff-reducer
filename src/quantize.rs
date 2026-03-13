pub fn quantize_f32_to_u8(input: &[f32], output: &mut [u8]) {
    let mut min = f32::MAX;
    let mut max = f32::MIN;

    for &val in input {
        if val < min { min = val; }
        if val > max { max = val; }
    }

    let range = max - min;
    if range == 0.0 {
        for val in output { *val = 0; }
        return;
    }

    for (i, &val) in input.iter().enumerate() {
        output[i] = (((val - min) / range) * 255.0) as u8;
    }
}

pub fn quantize_i16_to_u8(input: &[i16], output: &mut [u8]) {
    let mut min = i16::MAX;
    let mut max = i16::MIN;

    for &val in input {
        if val < min { min = val; }
        if val > max { max = val; }
    }

    let range = (max - min) as f32;
    if range == 0.0 {
        for val in output { *val = 0; }
        return;
    }

    for (i, &val) in input.iter().enumerate() {
        output[i] = (((val - min) as f32 / range) * 255.0) as u8;
    }
}
