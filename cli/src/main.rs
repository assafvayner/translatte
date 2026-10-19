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
            let mut last = String::new();
            let regions = pipeline.run(&img, src, tgt, &mut |s: Stage| {
                let name = format!("{:?}", s)
                    .split([' ', '{'])
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if name != last {
                    eprintln!("[{:>8.2?}] {name}", t1.elapsed());
                    last = name;
                }
            })?;
            eprintln!("[{:>8.2?}] done: {} regions", t1.elapsed(), regions.len());
            let json_text = serde_json::to_string_pretty(&regions)?;
            match json {
                Some(p) => std::fs::write(p, json_text)?,
                None => println!("{json_text}"),
            }
            let _ = output;
        }
    }
    Ok(())
}
