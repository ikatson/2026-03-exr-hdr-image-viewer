#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayHDR {
    pub sdr_white_nits: f32,
    pub peak_luma_nits: f32,
    pub nits_to_output_scale: f32,
}

impl Default for DisplayHDR {
    fn default() -> Self {
        Self {
            sdr_white_nits: 300.,
            peak_luma_nits: 300.,
            nits_to_output_scale: 1. / 300.,
        }
    }
}

// sdr white nits - windows only
// input white nits - windows only
//
// what I need is the ratio between them
// I also need the max display (relative to SDR white)
