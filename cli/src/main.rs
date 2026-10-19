mod draw;
mod eval;
mod fetch;
mod setup;

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use translatte_core::manifest::Language;
use translatte_core::pipeline::Stage;

#[derive(Parser)]
#[command(name = "translatte", about = "Offline photo translation (dev CLI)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct ModelArgs {
    #[arg(long, default_value = "models-cache")]
    models: PathBuf,
    #[arg(long, default_value = "models/manifest.json")]
    manifest: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// Measure OCR character error rate over a fixtures dir (see fixtures/README.md).
    Eval {
        #[arg(default_value = "fixtures")]
        dir: PathBuf,
        /// Exit non-zero if mean CER exceeds this.
        #[arg(long)]
        max_cer: Option<f64>,
        #[command(flatten)]
        models: ModelArgs,
    },
    /// Download the core models (detector, translator) and OCR script packs from upstream.
    Fetch {
        /// Script packs to install, e.g. rec-latin,rec-cjk.
        #[arg(long, value_delimiter = ',', conflicts_with = "all_packs")]
        packs: Vec<String>,
        /// Install every script pack in the manifest.
        #[arg(long)]
        all_packs: bool,
        #[command(flatten)]
        models: ModelArgs,
    },
    /// List languages available as source (installed packs) and target.
    Langs {
        #[command(flatten)]
        models: ModelArgs,
    },
    /// Translate the text in an image.
    Run {
        image: PathBuf,
        #[arg(long)]
        from: String,
        /// Target language; required unless --no-translate is set.
        #[arg(long)]
        to: Option<String>,
        #[arg(short, long, default_value = "out/translated.png")]
        output: PathBuf,
        #[arg(long)]
        json: Option<PathBuf>,
        /// OCR only; skip loading the LLM and outline the detected text boxes instead.
        #[arg(long)]
        no_translate: bool,
        #[command(flatten)]
        models: ModelArgs,
    },
}

fn main() {
    let code = match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e:?}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    // Bionic aborts in ONNX Runtime's C++ static destructors at exit; skip them on Android.
    #[cfg(target_os = "android")]
    unsafe {
        libc::_exit(code)
    };
    #[cfg(not(target_os = "android"))]
    std::process::exit(code);
}

/// Returns the process exit code.
fn real_main() -> Result<i32> {
    match Cli::parse().command {
        Command::Eval {
            dir,
            max_cer,
            models,
        } => {
            if !eval::run_eval(&dir, &models.manifest, &models.models, max_cer)? {
                return Ok(1);
            }
        }
        Command::Fetch {
            packs,
            all_packs,
            models,
        } => {
            let store = setup::load_store(&models.manifest, &models.models)?;
            fetch::run_fetch(&store, &packs, all_packs)?;
        }
        Command::Langs { models } => {
            let store = setup::load_store(&models.manifest, &models.models)?;
            let codes = |ls: &[&Language]| {
                ls.iter()
                    .map(|l| l.code.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            println!("source: {}", codes(&store.available_source_languages()?));
            println!("target: {}", codes(&store.target_languages()));
        }
        Command::Run {
            image,
            from,
            to,
            output,
            json,
            no_translate,
            models,
        } => {
            if to.is_none() && !no_translate {
                bail!("--to is required unless --no-translate is set");
            }
            let mut img = image::open(&image)
                .with_context(|| format!("opening {}", image.display()))?
                .to_rgb8();
            let store = setup::load_store(&models.manifest, &models.models)?;
            let src = store.manifest().language(&from)?;
            let tgt = match &to {
                Some(code) => store.manifest().language(code)?,
                None => src,
            };
            let t0 = Instant::now();
            let mut pipeline = setup::build_pipeline(&store, src, !no_translate)?;
            eprintln!("loaded models in {:.2?}", t0.elapsed());
            let t1 = Instant::now();
            let mut last = "";
            let output_data = pipeline.run(&img, src, tgt, &mut |s: Stage| {
                let name = s.name();
                if name != last {
                    eprintln!("[{:>8.2?}] {name}", t1.elapsed());
                    last = name;
                }
            })?;
            let regions = &output_data.regions;
            eprintln!("[{:>8.2?}] done: {} regions", t1.elapsed(), regions.len());
            if let Some(e) = &output_data.translation_error {
                eprintln!("translation failed: {e}");
            }
            let json_text = serde_json::to_string_pretty(&output_data)?;
            match json {
                Some(p) => {
                    if let Some(dir) = p.parent() {
                        std::fs::create_dir_all(dir)
                            .with_context(|| format!("creating {}", dir.display()))?;
                    }
                    std::fs::write(&p, json_text)
                        .with_context(|| format!("writing {}", p.display()))?
                }
                None => println!("{json_text}"),
            }
            if no_translate {
                draw::draw_outlines(&mut img, regions);
            } else {
                draw::Renderer::new().draw_regions(&mut img, regions);
            }
            if let Some(dir) = output.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            img.save(&output)
                .with_context(|| format!("writing {}", output.display()))?;
            eprintln!("wrote {}", output.display());
        }
    }
    Ok(0)
}
