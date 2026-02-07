use crate::AudioFormat;

pub(crate) const TARGET_SAMPLE_RATE: u32 = 16_000;

pub(crate) fn model_filename(model_name: &str) -> String {
    model_name.to_string()
}

pub(crate) fn prepare_audio(audio_data: &[f32], format: &AudioFormat) -> Vec<f32> {
    let _ = (audio_data, format);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_filename_maps_named_models() {
        assert_eq!(model_filename("base"), "ggml-base.bin");
        assert_eq!(model_filename("base.en"), "ggml-base.en.bin");
        assert_eq!(model_filename("small"), "ggml-small.bin");
    }

    #[test]
    fn model_filename_preserves_existing_bin_name() {
        assert_eq!(model_filename("ggml-custom.bin"), "ggml-custom.bin");
    }

    #[test]
    fn prepare_audio_downmixes_stereo_and_resamples_to_16k() {
        let input = vec![0.2, 0.6, 0.4, 0.8, 0.6, 1.0];
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        };

        let out = prepare_audio(&input, &format);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.4).abs() < 0.001);
    }

    #[test]
    fn prepare_audio_passthroughs_16k_mono() {
        let input = vec![0.1, -0.2, 0.4, -0.6];
        let format = AudioFormat {
            sample_rate: TARGET_SAMPLE_RATE,
            channels: 1,
            bits_per_sample: 16,
        };
        let out = prepare_audio(&input, &format);
        assert_eq!(out, input);
    }
}
