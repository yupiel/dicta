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
