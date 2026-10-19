use std::io::Write;
use std::sync::atomic::AtomicBool;

use anyhow::{bail, Result};
use translatte_core::download::{DownloadProgress, Downloader};
use translatte_core::manifest::{ModelKind, DETECTOR_ID, TRANSLATOR_ID};
use translatte_core::model_store::ModelStore;

/// Downloads the core models plus the requested script packs; verified files are skipped.
pub fn run_fetch(store: &ModelStore, packs: &[String], all_packs: bool) -> Result<()> {
    let manifest = store.manifest();
    let mut ids = vec![DETECTOR_ID, TRANSLATOR_ID];
    if all_packs {
        ids.extend(
            manifest
                .models
                .iter()
                .filter(|m| m.kind == ModelKind::ScriptPack)
                .map(|m| m.id.as_str()),
        );
    } else {
        for p in packs {
            match manifest.model(p) {
                Some(m) if m.kind == ModelKind::ScriptPack => {
                    if !ids.contains(&m.id.as_str()) {
                        ids.push(&m.id)
                    }
                }
                _ => bail!("{p} is not a script pack in the manifest"),
            }
        }
    }
    std::fs::create_dir_all(store.root())?;
    let downloader = Downloader::new(store)?;
    downloader.download_models_blocking(&ids, &AtomicBool::new(false), print_progress)?;
    eprintln!(
        "installed {} into {}",
        ids.join(" "),
        store.root().display()
    );
    Ok(())
}

fn print_progress(p: DownloadProgress) {
    const MIB: f64 = 1024.0 * 1024.0;
    let pct = (p.bytes_done * 100)
        .checked_div(p.bytes_total)
        .unwrap_or(100);
    let mut err = std::io::stderr().lock();
    let _ = write!(
        err,
        "\r{}: {pct:>3}% ({:.1}/{:.1} MiB)",
        p.file_path,
        p.bytes_done as f64 / MIB,
        p.bytes_total as f64 / MIB
    );
    if p.bytes_done >= p.bytes_total {
        let _ = writeln!(err);
    }
    let _ = err.flush();
}
