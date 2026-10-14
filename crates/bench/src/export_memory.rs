//! A full-size export's memory, stage by stage (PERFORMANCE §66):
//! `bench --cpus 4 --export-memory FILE [heavy] [tiff] [nolens]`.
//!
//! Memory is the process's physical footprint, what macOS counts against the machine
//! (Activity Monitor's Memory column), not its resident size: the allocator keeps
//! freed pages mapped and reusable, so resident size only ever grows.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use app_core::{
    EditRecipe, Engine, ExportColourSpace, ExportFormat, FileExport, MetadataChoice,
    OutputSharpening,
};
use serde_json::{Value, json};

/// The physical footprint now and at most so far, in MB (resident size elsewhere).
pub fn footprint_mb() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: proc_pid_rusage writes a rusage_info_v4 into the buffer given.
        let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            libc::proc_pid_rusage(
                std::process::id() as libc::c_int,
                libc::RUSAGE_INFO_V4,
                (&mut info as *mut libc::rusage_info_v4).cast(),
            )
        };
        if ok == 0 {
            return (
                info.ri_phys_footprint as f64 / 1e6,
                info.ri_lifetime_max_phys_footprint as f64 / 1e6,
            );
        }
        (0.0, 0.0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let peak = crate::measure::peak_rss_mb();
        (peak, peak)
    }
}

pub fn run(path: &Path, options: &[String]) -> Value {
    let has = |o: &str| options.iter().any(|x| x == o);
    let engine = Engine::new(crate::measure::engine_config());
    let mut recipe = EditRecipe::default();
    if has("heavy") {
        recipe.clarity = 25.0;
        recipe.texture = 20.0;
        recipe.noise_reduction = 30.0;
        recipe.dehaze = 20.0;
        recipe.grain = 15.0;
    }
    recipe.profile_corrections = !has("nolens");
    let tiff = has("tiff");
    let destination = std::env::temp_dir().join(format!(
        "pe-export-memory-{}.{}",
        std::process::id(),
        if tiff { "tif" } else { "jpg" }
    ));
    // The footprint's peak in each stage, sampled every few milliseconds.
    let stage = Arc::new(Mutex::new("Starting".to_owned()));
    let peaks: Arc<Mutex<Vec<(String, f64)>>> = Arc::default();
    let done = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (stage, peaks, done) = (Arc::clone(&stage), Arc::clone(&peaks), Arc::clone(&done));
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let now = footprint_mb().0;
                let name = stage.lock().expect("stage").clone();
                let mut peaks = peaks.lock().expect("peaks");
                match peaks.iter_mut().find(|(n, _)| *n == name) {
                    Some(p) => p.1 = p.1.max(now),
                    None => peaks.push((name, now)),
                }
                drop(peaks);
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
        })
    };
    let before = footprint_mb().0;
    let progress_stage = Arc::clone(&stage);
    let result = engine
        .export_file(
            FileExport {
                source: path.to_owned(),
                recipe,
                destination: destination.clone(),
                format: if tiff {
                    ExportFormat::Tiff
                } else {
                    ExportFormat::Jpeg { quality: 92 }
                },
                long_edge: None,
                sharpening: OutputSharpening::None,
                colour_space: ExportColourSpace::Srgb,
                metadata: MetadataChoice::All,
                judgements: Default::default(),
                watermark: None,
            },
            move |p| *progress_stage.lock().expect("stage") = format!("{:?}", p.stage),
        )
        .wait();
    done.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    let _ = std::fs::remove_file(&destination);
    let (after, peak) = footprint_mb();
    if has("pause") {
        // Time to look at what is still allocated (`vmmap`, `heap`).
        eprintln!("paused: pid {}", std::process::id());
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
    let stages: serde_json::Map<String, Value> = peaks
        .lock()
        .expect("peaks")
        .iter()
        .map(|(n, mb)| (n.clone(), json!(mb.round())))
        .collect();
    json!({
        "file": crate::measure::name(path),
        "options": options,
        "ok": result.is_ok(),
        "before_mb": before.round(),
        "peak_mb": peak.round(),
        "after_mb": after.round(),
        "stage_peaks_mb": stages,
    })
}
