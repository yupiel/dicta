use std::f64::consts::PI;

const ZERO_CROSSINGS: f64 = 8.0;
const PASSBAND_FRACTION: f64 = 0.95;

pub fn downmix_to_mono(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let scale = 1.0 / channels as f32;
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() * scale),
    );
}

/// Streaming windowed-sinc resampler.
///
/// When downsampling, the kernel doubles as the anti-aliasing low-pass filter.
/// Without it, everything above the output Nyquist frequency folds back into
/// the speech band and degrades recognition.
pub struct Resampler {
    step: f64,
    cutoff: f64,
    half_width: usize,
    buffer: Vec<f32>,
    position: f64,
    passthrough: bool,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        assert!(
            input_rate > 0 && output_rate > 0,
            "sample rates must be non-zero"
        );

        let ratio = f64::from(output_rate) / f64::from(input_rate);
        let cutoff = ratio.min(1.0) * PASSBAND_FRACTION;
        let half_width = (ZERO_CROSSINGS / cutoff).ceil() as usize;

        Self {
            step: 1.0 / ratio,
            cutoff,
            half_width,
            buffer: vec![0.0; half_width],
            position: half_width as f64,
            passthrough: input_rate == output_rate,
        }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.buffer.extend_from_slice(input);
        self.drain_ready(out);
    }

    /// Emits the samples that were held back waiting for look-ahead.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if self.passthrough {
            return;
        }

        self.buffer
            .extend(std::iter::repeat_n(0.0, self.half_width));

        self.drain_ready(out);
    }

    fn drain_ready(&mut self, out: &mut Vec<f32>) {
        while self.position.floor() as usize + self.half_width < self.buffer.len() {
            out.push(self.interpolate());
            self.position += self.step;
        }

        let consumed = (self.position.floor() as usize).saturating_sub(self.half_width);

        self.buffer.drain(..consumed);
        self.position -= consumed as f64;
    }

    fn interpolate(&self) -> f32 {
        let center = self.position.floor() as usize;
        let first = center + 1 - self.half_width;
        let last = center + self.half_width;
        let support = self.half_width as f64;

        let sum: f64 = (first..=last)
            .map(|index| {
                let distance = index as f64 - self.position;
                f64::from(self.buffer[index]) * kernel(distance, self.cutoff, support)
            })
            .sum();
        sum as f32
    }
}

fn kernel(distance: f64, cutoff: f64, support: f64) -> f64 {
    if distance.abs() >= support {
        return 0.0;
    }

    let phase = PI * cutoff * distance;
    let sinc = if phase == 0.0 {
        1.0
    } else {
        phase.sin() / phase
    };

    let ratio = distance / support;
    let blackman = 0.42 + 0.5 * (PI * ratio).cos() + 0.08 * (2.0 * PI * ratio).cos();

    cutoff * sinc * blackman
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let count = (rate as f32 * seconds) as usize;
        (0..count)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn resample_all(input: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
        let mut resampler = Resampler::new(input_rate, output_rate);
        let mut out = Vec::new();
        resampler.process(input, &mut out);
        resampler.finish(&mut out);
        out
    }

    fn steady_state_rms(samples: &[f32]) -> f32 {
        let middle = &samples[samples.len() / 10..samples.len() * 9 / 10];
        (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt()
    }

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn downmix_passes_mono_through() {
        let mut out = Vec::new();
        downmix_to_mono(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn output_length_matches_rate_ratio() {
        let out = resample_all(&sine(440.0, 48_000, 1.0, 0.5), 48_000, 16_000);
        assert!((out.len() as i64 - 16_000).abs() <= 2, "got {}", out.len());
    }

    #[test]
    fn upsampling_scales_length_up() {
        let out = resample_all(&sine(440.0, 8_000, 1.0, 0.5), 8_000, 16_000);
        assert!((out.len() as i64 - 16_000).abs() <= 2, "got {}", out.len());
    }

    #[test]
    fn preserves_tone_frequency() {
        let out = resample_all(&sine(440.0, 48_000, 1.0, 0.5), 48_000, 16_000);
        let crossings = out.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((crossings as i64 - 440).abs() <= 2, "got {crossings}");
    }

    #[test]
    fn in_band_tone_keeps_its_amplitude() {
        let out = resample_all(&sine(3_000.0, 48_000, 1.0, 0.5), 48_000, 16_000);
        let expected = 0.5 / 2.0_f32.sqrt();
        let actual = steady_state_rms(&out);
        assert!((actual - expected).abs() < 0.03 * expected, "got {actual}");
    }

    #[test]
    fn tone_above_output_nyquist_is_suppressed_instead_of_aliased() {
        let out = resample_all(&sine(12_000.0, 48_000, 1.0, 0.5), 48_000, 16_000);
        assert!(
            steady_state_rms(&out) < 0.005,
            "got {}",
            steady_state_rms(&out)
        );
    }

    #[test]
    fn chunked_processing_equals_single_pass() {
        let input = sine(300.0, 44_100, 0.5, 0.4);
        let whole = resample_all(&input, 44_100, 16_000);

        let mut resampler = Resampler::new(44_100, 16_000);
        let mut chunked = Vec::new();
        for chunk in input.chunks(97) {
            resampler.process(chunk, &mut chunked);
        }
        resampler.finish(&mut chunked);

        assert_eq!(whole.len(), chunked.len());
        for (a, b) in whole.iter().zip(&chunked) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn equal_rates_pass_samples_through_unchanged() {
        let input = sine(500.0, 16_000, 0.1, 0.3);
        assert_eq!(resample_all(&input, 16_000, 16_000), input);
    }
}
