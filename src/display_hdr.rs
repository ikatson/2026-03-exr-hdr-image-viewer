#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayHDR {
    pub sdr_white_vs_input: f32,
    pub peak_luma_vs_sdr_white: f32,
}

impl Default for DisplayHDR {
    fn default() -> Self {
        Self {
            sdr_white_vs_input: 1.,
            peak_luma_vs_sdr_white: 1.,
        }
    }
}

// sdr white nits - windows only
// input white nits - windows only
//
// what I need is the ratio between them
// I also need the max display (relative to SDR white)
