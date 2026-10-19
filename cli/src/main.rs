mod draw;
mod eval;
mod setup;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
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
        #[arg(long)]
        to: String,
        #[arg(short, long, default_value = "out/translated.png")]
        output: PathBuf,
        #[arg(long)]
        json: Option<PathBuf>,
        /// OCR only; skip loading the LLM.
        #[arg(long)]
        no_translate: bool,
        #[command(flatten)]
        models: ModelArgs,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Eval {
            dir,
            max_cer,
            models,
        } => {
            if !eval::run_eval(&dir, &models.manifest, &models.models, max_cer)? {
                std::process::exit(1);
            }
        }
        Command::Langs { models } => {
            let store = setup::load_store(&models.manifest, &models.models)?;
            let codes = |ls: Vec<&translatte_core::manifest::Language>| {
                ls.iter()
                    .map(|l| l.code.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            println!("source: {}", codes(store.available_source_languages()?));
            println!("target: {}", codes(store.target_languages()));
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
            let store = setup::load_store(&models.manifest, &models.models)?;
            let (src, tgt) = (
                store.manifest().language(&from)?,
                store.manifest().language(&to)?,
            );
            let t0 = Instant::now();
            let mut pipeline = setup::build_pipeline(&store, src, !no_translate)?;
            eprintln!("loaded models in {:.2?}", t0.elapsed());
            let img = image::open(&image)?.to_rgb8();
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
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(p, json_text)?
                }
                None => println!("{json_text}"),
            }
            let mut rendered = img.clone();
            draw::Renderer::new().draw_regions(&mut rendered, regions);
            if let Some(dir) = output.parent() {
                std::fs::create_dir_all(dir)?;
            }
            rendered.save(&output)?;
            eprintln!("wrote {}", output.display());
        }
    }
    Ok(())
}
