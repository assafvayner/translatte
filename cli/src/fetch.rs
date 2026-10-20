use std::io::Write;
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Result};
use translatte_core::download::{DownloadProgress, Downloader};
use translatte_core::manifest::{Language, DETECTOR_ID};
use translatte_core::model_store::ModelStore;

/// Removes model data the manifest no longer describes, then downloads the core models plus the
/// packs (script pack and pair model) of the given source languages; installed files are skipped.
pub fn run_fetch(store: &ModelStore, langs: &[String], all: bool) -> Result<()> {
    let sources = store.source_languages();
    let chosen: Vec<&Language> = if all {
        sources
    } else {
        langs
            .iter()
            .map(|code| {
                sources
                    .iter()
                    .copied()
                    .find(|l| &l.code == code)
                    .ok_or_else(|| anyhow!("{code} is not a source language in the manifest"))
            })
            .collect::<Result<_>>()?
    };
    let mut ids = vec![DETECTOR_ID];
    for l in chosen {
        for id in store.language_models(l) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    std::fs::create_dir_all(store.root())?;
    for path in store.prune()? {
        eprintln!("removed stale {}", path.display());
    }
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
