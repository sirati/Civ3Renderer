//! Exact DSP, attenuation, panning, pitch-table, and clamping math matching
//! Civilization III's `sound.dll` and DirectSound (`docs/audio-engine.md` §4).

/// Internal canonical mixing rate matching `sound.dll`'s DirectSound primary
/// buffer (`0x1001204D`).
pub const MIX_SAMPLE_RATE: u32 = 44_100;

/// Software mixer rate used by `sound.dll`'s secondary WAV mixing buffer
/// (`0x10006EF5`).
pub const SW_MIX_SAMPLE_RATE: u32 = 22_050;

/// DirectSound silence attenuation in hundredths of a decibel (`DSBVOLUME_MIN`).
pub const DSBVOLUME_MIN: i32 = -10_000;

/// Exact 25-entry semitone pitch-ratio table from `sound.dll` at `0x1005B6E8`
/// covering `-12` semitones (`0.5`) through `+12` semitones (`2.0`).
pub const SEMITONE_RATIO_TABLE: [f64; 25] = [
    0.5,
    0.529_731_547_179_6,
    0.561_231_024_154_7,
    0.594_603_557_501_4,
    0.629_960_524_947_4,
    0.667_419_927_085,
    0.707_106_781_186_5,
    0.749_153_538_438_3,
    0.793_700_525_984_1,
    0.840_896_415_253_7,
    0.890_898_718_140_3,
    0.943_874_312_681_7,
    1.0,
    1.059_463_094_359,
    1.122_462_048_309,
    1.189_207_115_003,
    1.259_921_049_895,
    1.334_839_854_17,
    1.414_213_562_373,
    1.498_307_076_877,
    1.587_401_051_968,
    1.681_792_830_507,
    1.781_797_436_281,
    1.887_748_625_363,
    2.0,
];

/// Converts a Civ3 volume level (`0..=127`) into `sound.dll`'s 16.16 fixed-point
/// software-mixer volume (`0..=65536`), matching `WaveSound::set_volume`
/// (`0x1003C280`).
#[inline]
pub fn sw_volume_fixed(vol: i32) -> i32 {
    let v = vol.clamp(0, 127);
    if v == 0 {
        0
    } else {
        (v + 1) << 9
    }
}

/// Converts a Civ3 volume (`0..=127`) and pan (`-64..=63`) into exact left and
/// right linear gains `[0.0, 1.0]`, matching `Inner_set_pan` (`0x10036A90`)
/// and `WaveSound::set_volume` (`0x1003C280`).
#[inline]
pub fn sw_pan_gains(vol: i32, pan: i32) -> (f64, f64) {
    let v = sw_volume_fixed(vol);
    sw_pan_gains_from_fixed(v, pan)
}

/// Same as [`sw_pan_gains`] taking a pre-scaled fixed-point volume in
/// `0..=65536` (used during linear fades).
#[inline]
pub fn sw_pan_gains_from_fixed(vol_fixed: i32, pan: i32) -> (f64, f64) {
    let v = vol_fixed.clamp(0, 65_536);
    let p = pan.clamp(-64, 63);
    let (vl, vr) = if p == 0 {
        (v, v)
    } else {
        let div = if p < 0 { 64 } else { 63 };
        let delta = (v * p) / div;
        ((v - delta).clamp(0, 65_536), (v + delta).clamp(0, 65_536))
    };
    const INV_65536: f64 = 1.0 / 65_536.0;
    (vl as f64 * INV_65536, vr as f64 * INV_65536)
}

/// Computes the exact DirectSound volume in hundredths of a dB (`mB`, in
/// `[-10000, 0]`) from sound volume (`0..=127`) and master volume (`0..=127`),
/// matching `DSBuf_set_vol` (`0x1000F430`):
///
/// ```text
/// combined = trunc((master / 127.0) * (vol / 127.0) * 127.0)
/// mB       = (combined == 0) ? -10000 : trunc(1000.0 * ln(combined / 127.0))
/// ```
#[inline]
pub fn ds_volume_mb(sound_vol: i32, master_vol: i32) -> i32 {
    let v = sound_vol.clamp(0, 127) as f64;
    let m = master_vol.clamp(0, 127) as f64;
    let combined = ((m / 127.0) * (v / 127.0) * 127.0).trunc() as i32;
    if combined <= 0 {
        DSBVOLUME_MIN
    } else {
        (1000.0 * (combined as f64 / 127.0).ln()).trunc() as i32
    }
}

/// Converts DirectSound hundredths of a dB (`mB`) into linear amplitude gain
/// `10^(mB / 2000)`, returning `0.0` at or below `DSBVOLUME_MIN`.
#[inline]
pub fn ds_mb_to_gain(mb: i32) -> f64 {
    if mb <= DSBVOLUME_MIN {
        0.0
    } else if mb >= 0 {
        1.0
    } else {
        10.0_f64.powf(mb as f64 / 2000.0)
    }
}

/// Computes the exact linear amplitude gain for a DirectSound streaming buffer
/// (MP3 music / playlist) at `(sound_vol, master_vol)`.
#[inline]
pub fn ds_volume_gain(sound_vol: i32, master_vol: i32) -> f64 {
    ds_mb_to_gain(ds_volume_mb(sound_vol, master_vol))
}

/// Computes left/right linear pan multipliers for DirectSound buffers when
/// `pan != 0` (DirectSound attenuates only the opposite channel).
#[inline]
pub fn ds_pan_gains(pan: i32) -> (f64, f64) {
    let p = pan.clamp(-64, 63);
    if p == 0 {
        return (1.0, 1.0);
    }
    if p < 0 {
        // Pan left -> attenuate right channel.
        let frac = (64 + p) as f64 / 64.0;
        let mb = if frac <= 0.0 { DSBVOLUME_MIN } else { (1000.0 * frac.ln()).trunc() as i32 };
        (1.0, ds_mb_to_gain(mb))
    } else {
        // Pan right -> attenuate left channel.
        let frac = (63 - p) as f64 / 63.0;
        let mb = if frac <= 0.0 { DSBVOLUME_MIN } else { (1000.0 * frac.ln()).trunc() as i32 };
        (ds_mb_to_gain(mb), 1.0)
    }
}

/// Computes the exact software-mixer pitch ratio from cents (`-1200..=1200`,
/// wrapping out-of-range values with `% 1200`), matching `WaveSound::set_pitch`
/// (`0x1003C3C0`).
#[inline]
pub fn pitch_ratio_from_cents(mut cents: i32) -> f64 {
    if !(-1200..=1200).contains(&cents) {
        cents %= 1200;
    }
    let semi = cents / 100;
    let rem = (cents % 100).abs();
    let idx = (semi + 12) as usize;
    let base = SEMITONE_RATIO_TABLE[idx];
    if rem == 0 {
        return base;
    }
    let next = if cents < 0 {
        SEMITONE_RATIO_TABLE[idx - 1]
    } else {
        SEMITONE_RATIO_TABLE[idx + 1]
    };
    base + (next - base) * (rem as f64) * 0.01
}

/// Rounds an `f64` mixed sample to `i16` with round-to-nearest-even (`fistp`)
/// and saturation to `[-32768, 32767]`, matching `Mixer_write_to_ds`
/// (`0x10007D19..0x10007D4B`).
#[inline]
pub fn clamp_i16(sample: f64) -> i16 {
    let rounded = sample.round_ties_even();
    if rounded >= 32_767.0 {
        32_767
    } else if rounded <= -32_768.0 {
        -32_768
    } else {
        rounded as i16
    }
}

/// Exact integer division by 127 used in `AmbSound::note_on` (`0x10021C56`).
#[inline]
pub fn amb_scale_127(a: i32, b: i32) -> i32 {
    (a * b) / 127
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sw_volume_and_pan_match_sound_dll() {
        assert_eq!(sw_volume_fixed(0), 0);
        assert_eq!(sw_volume_fixed(1), 1024);
        assert_eq!(sw_volume_fixed(64), 33_280);
        assert_eq!(sw_volume_fixed(127), 65_536);
        assert_eq!(sw_volume_fixed(200), 65_536);
        assert_eq!(sw_volume_fixed(-5), 0);

        // Center pan at max volume = (1.0, 1.0)
        assert_eq!(sw_pan_gains(127, 0), (1.0, 1.0));
        // Full left (-64): delta = -65536, V_R = 0, V_L = clamp(131072, 65536) = 65536
        assert_eq!(sw_pan_gains(127, -64), (1.0, 0.0));
        // Full right (+63): delta = +65536, V_L = 0, V_R = 65536
        assert_eq!(sw_pan_gains(127, 63), (0.0, 1.0));
        // Mid volume (64 -> V=33280), pan -32 -> delta = -16640 -> V_L=49920, V_R=16640
        let (l, r) = sw_pan_gains(64, -32);
        assert!((l - 49_920.0 / 65_536.0).abs() < 1e-12);
        assert!((r - 16_640.0 / 65_536.0).abs() < 1e-12);
    }

    #[test]
    fn ds_volume_uses_exact_natural_log_curve() {
        assert_eq!(ds_volume_mb(127, 127), 0);
        assert_eq!(ds_volume_mb(0, 127), DSBVOLUME_MIN);
        assert_eq!(ds_volume_mb(127, 0), DSBVOLUME_MIN);
        // ln(1/127)*1000 = -4844.187... -> trunc = -4844
        assert_eq!(ds_volume_mb(1, 127), -4844);
        // ln(64/127)*1000 = -685.30... -> trunc = -685
        assert_eq!(ds_volume_mb(64, 127), -685);
        assert!((ds_volume_gain(127, 127) - 1.0).abs() < 1e-12);
        assert_eq!(ds_volume_gain(0, 127), 0.0);
        assert!((ds_volume_gain(64, 127) - 10.0_f64.powf(-685.0 / 2000.0)).abs() < 1e-12);
    }

    #[test]
    fn pitch_ratio_matches_semitone_table_and_interpolation() {
        assert_eq!(pitch_ratio_from_cents(0), 1.0);
        assert_eq!(pitch_ratio_from_cents(1200), 2.0);
        assert_eq!(pitch_ratio_from_cents(-1200), 0.5);
        // +50 cents = halfway between 1.0 and 1.059463094359
        assert!((pitch_ratio_from_cents(50) - 1.029_731_547_179_5).abs() < 1e-12);
        // -50 cents = halfway between 1.0 and 0.9438743126817
        assert!((pitch_ratio_from_cents(-50) - 0.971_937_156_340_85).abs() < 1e-12);
        // Wrap >1200 via % 1200
        assert_eq!(pitch_ratio_from_cents(1500), pitch_ratio_from_cents(300));
    }

    #[test]
    fn clamp_i16_saturates_and_rounds_ties_to_even() {
        assert_eq!(clamp_i16(0.0), 0);
        assert_eq!(clamp_i16(2.5), 2);
        assert_eq!(clamp_i16(3.5), 4);
        assert_eq!(clamp_i16(40_000.0), 32_767);
        assert_eq!(clamp_i16(-40_000.0), -32_768);
    }
}
