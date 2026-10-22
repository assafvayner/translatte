mod bench;
mod draw;
mod eval;
mod fetch;
mod setup;

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use translatte_core::manifest::Language;
use translatte_core::ocr::Precision;
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
        /// OCR precision: fp32 or fp16.
        #[arg(long, default_value = "fp32")]
        precision: Precision,
        #[command(flatten)]
        models: ModelArgs,
    },
    /// Time the pipeline stages over a fixtures dir and report OCR CER.
    Bench {
        #[arg(default_value = "fixtures")]
        dir: PathBuf,
        #[arg(long, default_value_t = 5)]
        iters: usize,
        /// Downscale images so the long side is at most this, e.g. 960 for live camera frames.
        #[arg(long)]
        max_side: Option<u32>,
        /// Also translate (loads the pair models).
        #[arg(long)]
        translate: bool,
        /// Only fixtures in these source languages, e.g. fr,ja.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Write the recognized lines (text, confidence) per fixture to this JSON file.
        #[arg(long)]
        text: Option<PathBuf>,
        /// Run the fixtures, in name order, once each as consecutive live camera frames.
        #[arg(long)]
        live: bool,
        /// OCR precision: fp32 or fp16.
        #[arg(long, default_value = "fp32")]
        precision: Precision,
        #[command(flatten)]
        models: ModelArgs,
    },
    /// Download the core models and the packs (OCR script pack and translation model) of source languages.
    Fetch {
        /// Source languages to install, e.g. fr,ja.
        #[arg(long, value_delimiter = ',', conflicts_with = "all")]
        langs: Vec<String>,
        /// Install every source language in the manifest.
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        models: ModelArgs,
    },
    /// List the installed source languages and the target language.
    Langs {
        #[command(flatten)]
        models: ModelArgs,
    },
    /// Translate the text in an image into English.
    Run {
        image: PathBuf,
        #[arg(long)]
        from: String,
        #[arg(short, long, default_value = "out/translated.png")]
        output: PathBuf,
        #[arg(long)]
        json: Option<PathBuf>,
        /// OCR only; skip loading the translation model and outline the detected text boxes instead.
        #[arg(long)]
        no_translate: bool,
        /// OCR precision: fp32 or fp16.
        #[arg(long, default_value = "fp32")]
        precision: Precision,
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
    // Bionic aborted in ONNX Runtime's C++ static destructors at exit. ORT is gone, but exiting
    // through the remaining C++ libraries' static destructors is untested on Android; skip them.
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
            precision,
            models,
        } => {
            if !eval::run_eval(&dir, &models.manifest, &models.models, max_cer, precision)? {
                return Ok(1);
            }
        }
        Command::Bench {
            dir,
            iters,
            max_side,
            translate,
            only,
            text,
            live,
            precision,
            models,
        } => {
            let args = bench::BenchArgs {
                dir,
                iters,
                max_side,
                translate,
                only,
                text,
                live,
                precision,
            };
            bench::run_bench(&args, &models.manifest, &models.models)?;
        }
        Command::Fetch { langs, all, models } => {
            let store = setup::load_store(&models.manifest, &models.models)?;
            fetch::run_fetch(&store, &langs, all)?;
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
            println!("target: {}", store.target_language()?.code);
        }
        Command::Run {
            image,
            from,
            output,
            json,
            no_translate,
            precision,
            models,
        } => {
            let mut img = image::open(&image)
                .with_context(|| format!("opening {}", image.display()))?
                .to_rgb8();
            let store = setup::load_store(&models.manifest, &models.models)?;
            let src = store.manifest().language(&from)?;
            let tgt = store.target_language()?;
            let t0 = Instant::now();
            let pipeline = setup::build_pipeline(&store, src, !no_translate, precision)?;
            eprintln!("loaded models in {:.2?}", t0.elapsed());
            if !no_translate {
                let t = Instant::now();
                pipeline.warm(src, tgt)?;
                eprintln!(
                    "loaded and warmed the translation model in {:.2?}",
                    t.elapsed()
                );
            }
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
