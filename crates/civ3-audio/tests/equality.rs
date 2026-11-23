//! Deterministic numerical equality test comparing our native `civ3-audio`
//! engine against the real original 32-bit `sound.dll` + `Mss32.dll` running
//! under Wine (`crates/civ3-sound-dll/examples/equality_capture.rs`).
//!
//! Run with:
//!   cargo test -p civ3-audio --test equality -- --nocapture

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use civ3_audio::mixer::{AudioEngine, SNDERR_OK};
use civ3_audio::wire::{Op, RequestHeader, SoundKind};

const ORIG_CONQUESTS: &str = "/mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests";
const STEP_FRAMES: usize = 882; // 20 ms at 44100 Hz

#[derive(Debug)]
struct CompareMetrics {
    lag_frames: isize,
    corr: f64,
    snr_db: f64,
    rms_orig: f64,
    rms_native: f64,
    rms_ratio: f64,
    rms_err: f64,
    max_abs_err: i32,
}

fn read_wav_i16_stereo(path: &Path) -> Vec<i16> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    assert!(bytes.len() >= 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE");
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let tag = &bytes[off..off + 4];
        let size = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as usize;
        off += 8;
        if tag == b"data" {
            let end = (off + size).min(bytes.len());
            let data = &bytes[off..end];
            let mut out = Vec::with_capacity(data.len() / 2);
            for chunk in data.chunks_exact(2) {
                out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
            }
            return out;
        }
        off = off.saturating_add(size + (size & 1));
    }
    panic!("no data chunk in {}", path.display());
}

fn first_nonzero_frame(pcm: &[i16]) -> usize {
    for (i, pair) in pcm.chunks_exact(2).enumerate() {
        if pair[0].abs() > 8 || pair[1].abs() > 8 {
            return i;
        }
    }
    0
}

/// Aligns two 44100 Hz interleaved stereo `i16` waveforms by onset-guided
/// cross-correlation within `±search_radius` frames around the initial onset
/// offset, then computes SNR, RMS error, max-abs error, and level ratio.
fn compare_stereo_pcm(orig: &[i16], native: &[i16], search_radius: isize) -> CompareMetrics {
    let n_orig = orig.len() / 2;
    let n_nat = native.len() / 2;
    let center_lag = first_nonzero_frame(orig) as isize - first_nonzero_frame(native) as isize;
    let lo_bound = center_lag - search_radius;
    let hi_bound = center_lag + search_radius;

    let corr_at = |lag: isize, stride: usize| -> f64 {
        let o_start = lag.max(0) as usize;
        let n_start = (-lag).max(0) as usize;
        let len = n_orig.saturating_sub(o_start).min(n_nat.saturating_sub(n_start));
        let mut dot = 0.0_f64;
        let mut pa = 0.0_f64;
        let mut pb = 0.0_f64;
        let mut i = 0;
        while i < len {
            let oi = (o_start + i) * 2;
            let ni = (n_start + i) * 2;
            let al = f64::from(orig[oi]);
            let ar = f64::from(orig[oi + 1]);
            let bl = f64::from(native[ni]);
            let br = f64::from(native[ni + 1]);
            dot += al * bl + ar * br;
            pa += al * al + ar * ar;
            pb += bl * bl + br * br;
            i += stride;
        }
        if pa > 0.0 && pb > 0.0 {
            dot / (pa * pb).sqrt()
        } else {
            f64::NEG_INFINITY
        }
    };

    // Fast coarse search (step 16, decimated 4x), then exact 1-frame refinement.
    let mut best_lag = center_lag;
    let mut best_corr = f64::NEG_INFINITY;
    let mut lag = lo_bound;
    while lag <= hi_bound {
        let c = corr_at(lag, 4);
        if c > best_corr {
            best_corr = c;
            best_lag = lag;
        }
        lag += 16;
    }
    let refine_lo = (best_lag - 32).max(lo_bound);
    let refine_hi = (best_lag + 32).min(hi_bound);
    let mut best_exact = f64::NEG_INFINITY;
    for l in refine_lo..=refine_hi {
        let c = corr_at(l, 1);
        if c > best_exact {
            best_exact = c;
            best_lag = l;
        }
    }

    let o_start = best_lag.max(0) as usize;
    let n_start = (-best_lag).max(0) as usize;
    let len = n_orig.saturating_sub(o_start).min(n_nat.saturating_sub(n_start));

    let mut sig_p = 0.0_f64;
    let mut nat_p = 0.0_f64;
    let mut err_p = 0.0_f64;
    let mut dot = 0.0_f64;
    let mut max_abs = 0i32;
    let samples = (len * 2) as f64;

    for i in 0..len {
        let oi = (o_start + i) * 2;
        let ni = (n_start + i) * 2;
        for ch in 0..2 {
            let a = f64::from(orig[oi + ch]);
            let b = f64::from(native[ni + ch]);
            sig_p += a * a;
            nat_p += b * b;
            let d = a - b;
            err_p += d * d;
            dot += a * b;
            max_abs = max_abs.max((i32::from(orig[oi + ch]) - i32::from(native[ni + ch])).abs());
        }
    }

    let corr = if sig_p > 0.0 && nat_p > 0.0 {
        dot / (sig_p * nat_p).sqrt()
    } else {
        0.0
    };
    let snr_db = if err_p <= 1e-12 {
        f64::INFINITY
    } else if sig_p <= 1e-12 {
        0.0
    } else {
        10.0 * (sig_p / err_p).log10()
    };
    let rms_orig = (sig_p / samples.max(1.0)).sqrt();
    let rms_native = (nat_p / samples.max(1.0)).sqrt();
    let rms_ratio = if rms_orig > 1e-9 { rms_native / rms_orig } else { 0.0 };
    let rms_err = (err_p / samples.max(1.0)).sqrt();

    CompareMetrics {
        lag_frames: best_lag,
        corr,
        snr_db,
        rms_orig,
        rms_native,
        rms_ratio,
        rms_err,
        max_abs_err: max_abs,
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn win_z_path(p: &Path) -> String {
    format!("Z:{}", p.to_str().unwrap().replace('/', "\\"))
}

fn render_native_scenario(
    conquests: &Path,
    synth_8bit: &Path,
    synth_stereo: &Path,
    det_amb: &Path,
    name: &str,
) -> Vec<i16> {
    let mut engine = AudioEngine::new();
    engine.set_base_dir(conquests);
    engine.set_rng_seed(0xC1F3);
    engine.handle_request(RequestHeader::new(Op::MidiOpen, 0), "");
    engine.handle_request(RequestHeader::new(Op::WaveOpen, 0), "");
    engine.handle_request(RequestHeader::new(Op::InitTimer, 0), "");

    let render_steps = |eng: &mut AudioEngine, steps: usize, mut per_step: Box<dyn FnMut(&mut AudioEngine, usize)>| {
        let mut out = vec![0i16; steps * STEP_FRAMES * 2];
        for step in 0..steps {
            per_step(eng, step);
            let s = step * STEP_FRAMES * 2;
            eng.render_frames(&mut out[s..s + STEP_FRAMES * 2]);
        }
        out
    };

    let create = |eng: &mut AudioEngine, kind: SoundKind| -> u32 {
        let r = eng.handle_request(RequestHeader::new(Op::CreateSound, 0).with_a(kind as i32), "");
        assert_eq!(r.snderr, SNDERR_OK);
        r.value
    };

    match name {
        "wav_mono16_unity" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Button1.wav").snderr, 0);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Play, h), "").snderr, 0);
            render_steps(&mut engine, 70, Box::new(|_, _| {}))
        }
        "wav_mono16_vol64_pan_l32" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Button1.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetVolume, h).with_a(64), "");
            engine.handle_request(RequestHeader::new(Op::SetPan, h).with_a(-32), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 70, Box::new(|_, _| {}))
        }
        "wav_mono16_vol64_pan_r32" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Button1.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetVolume, h).with_a(64), "");
            engine.handle_request(RequestHeader::new(Op::SetPan, h).with_a(32), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 70, Box::new(|_, _| {}))
        }
        "wav_mono16_pitch_p500" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Button1.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetPitch, h).with_a(500), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 70, Box::new(|_, _| {}))
        }
        "wav_mono16_pitch_m500" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Button1.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetPitch, h).with_a(-500), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 80, Box::new(|_, _| {}))
        }
        "wav_mono16_loop" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Button1.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetLoop, h).with_a(1), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 90, Box::new(|_, _| {}))
        }
        "wav_two_voice_mix" => {
            let h1 = create(&mut engine, SoundKind::Wave);
            let h2 = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h1), r"..\Sounds\Button1.wav").snderr, 0);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h2), r"..\Sounds\EnterTurn.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetVolume, h1).with_a(90), "");
            engine.handle_request(RequestHeader::new(Op::SetPan, h1).with_a(-20), "");
            engine.handle_request(RequestHeader::new(Op::SetVolume, h2).with_a(80), "");
            engine.handle_request(RequestHeader::new(Op::SetPan, h2).with_a(20), "");
            engine.handle_request(RequestHeader::new(Op::Play, h1), "");
            engine.handle_request(RequestHeader::new(Op::Play, h2), "");
            render_steps(&mut engine, 90, Box::new(|_, _| {}))
        }
        "wav_8bit_mono" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), synth_8bit.to_str().unwrap()).snderr, 0);
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 50, Box::new(|_, _| {}))
        }
        "wav_stereo16_sw_vol96_pan_l16" => {
            let h = create(&mut engine, SoundKind::Wave);
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), synth_stereo.to_str().unwrap()).snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetVolume, h).with_a(96), "");
            engine.handle_request(RequestHeader::new(Op::SetPan, h).with_a(-16), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 35, Box::new(|_, _| {}))
        }
        "wav_stereo16_stream_vol96" => {
            let h = create(&mut engine, SoundKind::Wave);
            engine.handle_request(RequestHeader::new(Op::SetFlags, h).with_a(0x100), "");
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\Barbarian Raid.wav").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetVolume, h).with_a(96), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 65, Box::new(|_, _| {}))
        }
        "mp3_unity" => {
            let h = create(&mut engine, SoundKind::Wave);
            engine.handle_request(RequestHeader::new(Op::SetFlags, h).with_a(0x100), "");
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\build\ancient\AncECfull.mp3").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 120, Box::new(|_, _| {}))
        }
        "mp3_vol64" => {
            let h = create(&mut engine, SoundKind::Wave);
            engine.handle_request(RequestHeader::new(Op::SetFlags, h).with_a(0x100), "");
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\build\ancient\AncECfull.mp3").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::SetVolume, h).with_a(64), "");
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 120, Box::new(|_, _| {}))
        }
        "mp3_fadeout" => {
            let h = create(&mut engine, SoundKind::Wave);
            engine.handle_request(RequestHeader::new(Op::SetFlags, h).with_a(0x100), "");
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Load, h), r"..\Sounds\build\ancient\AncECfull.mp3").snderr, 0);
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 100, Box::new(move |eng, step| {
                if step == 20 {
                    eng.handle_request(
                        RequestHeader::new(Op::Fade, h).with_a(0).with_b(-1).with_c(1000),
                        "",
                    );
                }
            }))
        }
        "amb_cavalry_det" => {
            let h = create(&mut engine, SoundKind::Amb);
            engine.handle_request(RequestHeader::new(Op::AmbSyncMeta, h).with_a(3).with_b(60), "");
            assert_eq!(
                engine.handle_request(
                    RequestHeader::new(Op::Load, h).with_a(3),
                    det_amb.to_str().unwrap(),
                ).snderr,
                0
            );
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            let mut out = vec![0i16; 30 * 2646 * 2];
            for step in 0..30usize {
                engine.handle_request(
                    RequestHeader::new(Op::AmbSetAnimFrame, h)
                        .with_a(step as i32)
                        .with_b(3)
                        .with_c(60),
                    "",
                );
                let s = step * 2646 * 2;
                engine.render_frames(&mut out[s..s + 2646 * 2]);
            }
            out
        }
        "amb_cavalry_attack" => {
            let h = create(&mut engine, SoundKind::Amb);
            engine.handle_request(RequestHeader::new(Op::AmbSyncMeta, h).with_a(3).with_b(66), "");
            assert_eq!(
                engine.handle_request(
                    RequestHeader::new(Op::Load, h).with_a(3),
                    r"..\Art\Units\Cavalry\CavalryAttack.amb"
                ).snderr,
                0
            );
            engine.handle_request(RequestHeader::new(Op::Play, h), "");
            render_steps(&mut engine, 90, Box::new(move |eng, step| {
                let frame = ((step * 20) / 66) as i32;
                eng.handle_request(
                    RequestHeader::new(Op::AmbSetAnimFrame, h)
                        .with_a(frame)
                        .with_b(3)
                        .with_c(66),
                    "",
                );
            }))
        }
        "playlist_mp3_vol80" => {
            let h = create(&mut engine, SoundKind::Playlist);
            assert_eq!(
                engine.handle_request(
                    RequestHeader::new(Op::PlaylistQueueTrack, h),
                    r"..\Sounds\build\ancient\AncECfull.mp3",
                ).snderr,
                0
            );
            engine.handle_request(RequestHeader::new(Op::PlaylistSetScaledVolume, h).with_a(80), "");
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Play, h), "").snderr, 0);
            render_steps(&mut engine, 120, Box::new(|_, _| {}))
        }
        other => panic!("unknown scenario {other}"),
    }
}

#[test]
fn numerical_equality_against_original_sound_dll_and_mss32() {
    let conquests = Path::new(ORIG_CONQUESTS);
    if !conquests.join("sound.dll").is_file() {
        eprintln!("skipping equality test: original game assets not present at {ORIG_CONQUESTS}");
        return;
    }
    let root = repo_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let capture_exe = target.join("i686-pc-windows-gnu/release/examples/equality_capture.exe");
    if !capture_exe.is_file() {
        eprintln!(
            "skipping equality test: build capture driver first via `cargo build --release --example equality_capture -p civ3-sound-dll --target i686-pc-windows-gnu`"
        );
        return;
    }

    let out_dir = std::env::temp_dir().join(format!("civ3-audio-eq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(root.join("tools/wine-run"))
        .arg(&capture_exe)
        .arg(win_z_path(&conquests.join("sound.dll")))
        .arg(win_z_path(conquests))
        .arg(win_z_path(&out_dir))
        .arg("1")
        .output()
        .expect("run tools/wine-run equality_capture.exe");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("--- original sound.dll capture stdout ---\n{stdout}");
    if !output.status.success() {
        panic!("equality_capture failed ({}):\nstderr:\n{stderr}", output.status);
    }
    assert!(stdout.contains("equality_capture_done=1"), "capture incomplete:\n{stdout}\n{stderr}");
    // Under Linux Wine 80-bit x87, `127 * f64(1/127) < 1.0` truncates 64 -> 63 (-701 mB);
    // under 53-bit Windows x87 / IEEE f64, it yields -685 mB.
    assert!(
        stdout.contains("mp3_vol64_mb=Some(-701)") || stdout.contains("mp3_vol64_mb=Some(-685)"),
        "expected DirectSound sound.dll DSBuf_set_vol(64, 127) in {{-701, -685}} mB, got:\n{stdout}"
    );

    let synth_8bit = out_dir.join("synth_8bit.wav");
    let synth_stereo = out_dir.join("synth_stereo16.wav");
    let det_amb = out_dir.join("CavalryAttack_det.amb");

    // (name, min_corr, min_snr_db, max_lag, max_compare_frames_opt, rms_env_only)
    let scenarios: &[(&str, f64, f64, isize, Option<usize>, bool)] = &[
        ("wav_mono16_unity", 0.999, 40.0, 2048, None, false),
        ("wav_mono16_vol64_pan_l32", 0.999, 40.0, 2048, None, false),
        ("wav_mono16_vol64_pan_r32", 0.999, 40.0, 2048, None, false),
        ("wav_mono16_pitch_p500", 0.995, 25.0, 2048, None, false),
        ("wav_mono16_pitch_m500", 0.995, 25.0, 2048, None, false),
        ("wav_mono16_loop", 0.995, 25.0, 2048, None, false),
        ("wav_two_voice_mix", 0.995, 25.0, 2048, None, false),
        ("wav_8bit_mono", 0.999, 40.0, 2048, None, false),
        // Original sound.dll's MixKernel_16bit_stereo_copy (0x1001abb4) skips `add esi, 4`
        // on the last sample of every 441-frame tick, drifting 1 sample per 20ms block.
        // Compare the clean initial block (880 output frames) on the SW mixer path, plus
        // a full 1.6s real stereo WAV on the bug-free DirectSound streaming path (0x100).
        ("wav_stereo16_sw_vol96_pan_l16", 0.999, 40.0, 64, Some(880), false),
        ("wav_stereo16_stream_vol96", 0.995, 25.0, 2048, None, false),
        ("mp3_unity", 0.990, 20.0, 2048, None, false),
        ("mp3_vol64", 0.990, 20.0, 2048, None, false),
        ("mp3_fadeout", 0.980, 15.0, 2048, None, false),
        ("playlist_mp3_vol80", 0.990, 20.0, 2048, None, false),
        ("amb_cavalry_det", 0.995, 20.0, 4096, None, false),
        // Unmodified CavalryAttack.amb sets PRGM_RAND_PITCH (±200 cents via MSVC rand()),
        // so phase differs across RNGs while RMS energy & envelope match closely.
        ("amb_cavalry_attack", 0.85, 0.0, 4096, None, true),
    ];

    println!("| Scenario | Lag (frames) | Corr | SNR (dB) | RMS orig | RMS nat | Ratio | RMS err | Max abs |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");

    let mut failures = Vec::new();
    for &(name, min_corr, min_snr, max_lag, cap_frames, env_only) in scenarios {
        let orig_wav = out_dir.join(format!("{name}.wav"));
        assert!(orig_wav.is_file(), "missing original capture {}", orig_wav.display());
        let orig = read_wav_i16_stereo(&orig_wav);
        let nat = render_native_scenario(conquests, &synth_8bit, &synth_stereo, &det_amb, name);
        let m = if let Some(limit) = cap_frames {
            let fnz_o = first_nonzero_frame(&orig).saturating_sub(1);
            let o_end = (fnz_o + limit).min(orig.len() / 2);
            let n_end = limit.min(nat.len() / 2);
            compare_stereo_pcm(&orig[fnz_o * 2..o_end * 2], &nat[..n_end * 2], max_lag)
        } else {
            compare_stereo_pcm(&orig, &nat, max_lag)
        };
        let metric_corr = if env_only {
            rms_envelope_corr(&orig, &nat, m.lag_frames, 2205)
        } else {
            m.corr
        };
        println!(
            "| {name} | {} | {:.6} | {:.2} | {:.1} | {:.1} | {:.4} | {:.2} | {} |",
            m.lag_frames, metric_corr, m.snr_db, m.rms_orig, m.rms_native, m.rms_ratio, m.rms_err, m.max_abs_err
        );
        if m.rms_orig <= 10.0 {
            failures.push(format!("{name}: original output was silent (rms={})", m.rms_orig));
        } else if m.rms_native <= 10.0 {
            failures.push(format!("{name}: native output was silent (rms={})", m.rms_native));
        } else if env_only {
            if metric_corr < min_corr || !(0.85..=1.15).contains(&m.rms_ratio) {
                failures.push(format!(
                    "{name}: env_corr={metric_corr:.4} (< {min_corr}) or rms_ratio={:.4} out of [0.85, 1.15]",
                    m.rms_ratio
                ));
            }
        } else if m.corr < min_corr {
            failures.push(format!(
                "{name}: correlation {:.6} < {min_corr} (lag={}, snr={:.2} dB, rms_ratio={:.4})",
                m.corr, m.lag_frames, m.snr_db, m.rms_ratio
            ));
        } else if min_snr > 0.0 && m.snr_db < min_snr {
            failures.push(format!(
                "{name}: SNR {:.2} dB < {min_snr} dB (lag={}, corr={:.6}, rms_err={:.2})",
                m.snr_db, m.lag_frames, m.corr, m.rms_err
            ));
        }
    }

    let _ = std::fs::remove_dir_all(&out_dir);
    assert!(failures.is_empty(), "equality failures:\n{}", failures.join("\n"));
}

fn rms_envelope_corr(orig: &[i16], native: &[i16], lag: isize, window_frames: usize) -> f64 {
    let o_start = lag.max(0) as usize;
    let n_start = (-lag).max(0) as usize;
    let len = (orig.len() / 2)
        .saturating_sub(o_start)
        .min((native.len() / 2).saturating_sub(n_start));
    let mut eo = Vec::new();
    let mut en = Vec::new();
    let mut i = 0;
    while i + window_frames <= len {
        let mut so = 0.0;
        let mut sn = 0.0;
        for j in 0..window_frames {
            let oi = (o_start + i + j) * 2;
            let ni = (n_start + i + j) * 2;
            so += f64::from(orig[oi]).powi(2) + f64::from(orig[oi + 1]).powi(2);
            sn += f64::from(native[ni]).powi(2) + f64::from(native[ni + 1]).powi(2);
        }
        eo.push((so / (2 * window_frames) as f64).sqrt());
        en.push((sn / (2 * window_frames) as f64).sqrt());
        i += window_frames;
    }
    let dot: f64 = eo.iter().zip(en.iter()).map(|(a, b)| a * b).sum();
    let pa: f64 = eo.iter().map(|a| a * a).sum();
    let pb: f64 = en.iter().map(|b| b * b).sum();
    if pa > 0.0 && pb > 0.0 { dot / (pa * pb).sqrt() } else { 0.0 }
}

#[test]
fn end_to_end_32bit_sound_dll_via_wine_wow64_unixlib() {
    let conquests = Path::new(ORIG_CONQUESTS);
    if !conquests.join("sound.dll").is_file() {
        eprintln!("skipping e2e Wine WoW64 test: game assets missing");
        return;
    }
    let root = repo_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let capture_exe = target.join("i686-pc-windows-gnu/release/examples/equality_capture.exe");
    let sound_dll = target.join("i686-pc-windows-gnu/release/sound.dll");
    let native_dll = target.join("i686-pc-windows-gnu/release/civ3r_native.dll");
    let native_so = target.join("release/libciv3r_native.so");
    if !(capture_exe.is_file() && sound_dll.is_file() && native_dll.is_file() && native_so.is_file()) {
        eprintln!("skipping e2e Wine WoW64 test: release artifacts not built yet");
        return;
    }
    let wine = std::env::var_os("HOME")
        .map(|h| Path::new(&h).join(".cache/winvm/wine-wow64"))
        .and_then(|p| std::fs::canonicalize(p).ok())
        .map(|d| d.join("bin/wine"))
        .filter(|p| p.is_file());
    let Some(wine) = wine else {
        eprintln!("skipping e2e Wine WoW64 test: ~/.cache/winvm/wine-wow64 not found");
        return;
    };

    let tmp = std::env::temp_dir().join(format!("civ3-audio-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dllpath = tmp.join("native");
    let bin_dir = tmp.join("bin");
    let cap_dir = tmp.join("cap");
    std::fs::create_dir_all(dllpath.join("i386-windows")).unwrap();
    std::fs::create_dir_all(dllpath.join("x86_64-unix")).unwrap();
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&cap_dir).unwrap();

    let staged_dll = dllpath.join("i386-windows/civ3r_native.dll");
    std::fs::copy(&native_dll, &staged_dll).unwrap();
    let mark = Command::new(root.join("tools/mark-wine-builtin"))
        .arg(&staged_dll)
        .status()
        .expect("run tools/mark-wine-builtin");
    assert!(mark.success(), "mark-wine-builtin failed");
    std::fs::copy(&native_so, dllpath.join("x86_64-unix/civ3r_native.so")).unwrap();
    std::fs::copy(&staged_dll, bin_dir.join("civ3r_native.dll")).unwrap();
    std::fs::copy(&sound_dll, bin_dir.join("sound.dll")).unwrap();
    std::fs::copy(&capture_exe, bin_dir.join("equality_capture.exe")).unwrap();

    let mut cmd = Command::new(&wine);
    let prefix = tmp.join("prefix");
    std::fs::create_dir_all(&prefix).unwrap();
    cmd.arg(bin_dir.join("equality_capture.exe"))
        .arg(win_z_path(&bin_dir.join("sound.dll")))
        .arg(win_z_path(conquests))
        .arg(win_z_path(&cap_dir))
        .arg("0")
        .env("WINEPREFIX", &prefix)
        .env("WINEDLLPATH", &dllpath)
        .env("WINEDLLOVERRIDES", "civ3r_native=b;mscoree,mshtml=;")
        .env("WINEDEBUG", "-all")
        .env("CIV3R_SOUND", "native")
        .env("CIV3R_AUDIO_NULL", "1")
        .env_remove("DISPLAY");
    let output = cmd.output().expect("run wine-wow64 e2e capture");
    if let Some(ws) = wine.parent().map(|b| b.join("wineserver")).filter(|p| p.is_file()) {
        let _ = Command::new(ws).arg("-k").env("WINEPREFIX", &prefix).status();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("--- native 32-bit sound.dll -> 64-bit civ3r_native.so e2e stdout ---\n{stdout}");
    if !output.status.success() {
        panic!("native e2e failed ({}):\nstderr:\n{stderr}", output.status);
    }
    assert!(stdout.contains("version=0x05000d01"), "unexpected version:\n{stdout}");
    assert!(stdout.contains("equality_capture_done=1"), "incomplete:\n{stdout}");

    for name in ["wav_mono16_unity", "mp3_unity", "amb_cavalry_attack", "playlist_mp3_vol80"] {
        let p = cap_dir.join(format!("{name}.wav"));
        let pcm = read_wav_i16_stereo(&p);
        let rms = (pcm.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>()
            / (pcm.len().max(1) as f64))
            .sqrt();
        println!("e2e {name}: frames={} rms={rms:.1}", pcm.len() / 2);
        assert!(rms > 10.0, "e2e {name} rendered silence (rms={rms})");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}
