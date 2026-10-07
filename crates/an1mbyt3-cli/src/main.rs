//! `an1mbyt3-cli`: compile, optimize, and validate.
//!
//! See AN1MBYT3_BUILD_PLAN.md §2 and §7 ("File & Runtime Pipeline") — this is the
//! Rust-based compiler that turns `.tea` (human-readable source) into `.teab`
//! (compact production binary). Implemented so far: `compile` (`.tea`/`.teab`
//! -> `.teab`), `validate` (load either format, print a summary), and
//! `optimize` (redundant-keyframe stripping — see `optimize.rs`'s module
//! doc). Still not built: geometry dedup, string tables, unused-node
//! stripping, and `export`/`package` (package's actual meaning needs
//! product-level clarification, since font bytes already travel inline with
//! a `Document` — see `an1mbyt3-format`'s module doc — so there's no
//! separate "bundle the assets together" step left to do yet).
//!
//! Does NOT own: the actual document/format types (see `an1mbyt3-core`,
//! `an1mbyt3-format`) — this crate is the command-line entry point that
//! drives those crates.

mod optimize;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use an1mbyt3_core::Document;

#[derive(Parser)]
#[command(name = "an1mbyt3-cli", about = "an1mbyt3 document compiler/toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compiles a `.tea` (JSON source) file into a compact `.teab` binary.
    Compile {
        /// Path to the input `.tea` file.
        input: PathBuf,
        /// Path to write the output `.teab` file. Defaults to `input` with
        /// its extension replaced by `.teab`.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Loads a `.tea` or `.teab` file and prints a summary — proves the file
    /// actually decodes, without needing a full runtime/renderer to check.
    Validate {
        /// Path to the `.tea` or `.teab` file to check, distinguished by
        /// its extension (anything not literally `.teab` is read as `.tea`
        /// JSON).
        input: PathBuf,
    },
    /// Strips redundant keyframes (see `optimize.rs`'s module doc for the
    /// exact rule) and re-saves the result — a pre-encoding compaction pass,
    /// not a separate file format.
    Optimize {
        /// Path to the input `.tea` or `.teab` file.
        input: PathBuf,
        /// Path to write the optimized output. Defaults to overwriting `input`.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// How close a keyframe's value has to be to what its neighbors
        /// would already reconstruct before it's considered redundant, in
        /// that property's own units (world units for translation/scale,
        /// radians for rotation, 0..1 RGBA channel value for fill color).
        #[arg(short, long, default_value_t = 0.01)]
        tolerance: f32,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Compile { input, output } => compile(input, output.as_deref()),
        Command::Validate { input } => validate(input),
        Command::Optimize { input, output, tolerance } => optimize(input, output.as_deref(), *tolerance),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("an1mbyt3-cli: {message}");
            ExitCode::FAILURE
        }
    }
}

fn load_document_by_extension(path: &Path) -> Result<Document, String> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("teab") {
        let bytes = std::fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        an1mbyt3_format::load_teab(&bytes).map_err(|err| format!("failed to load {}: {err}", path.display()))
    } else {
        let json = std::fs::read_to_string(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        an1mbyt3_format::load_tea(&json).map_err(|err| format!("failed to load {}: {err}", path.display()))
    }
}

fn compile(input: &Path, output: Option<&Path>) -> Result<(), String> {
    let document = load_document_by_extension(input)?;

    let teab_bytes = an1mbyt3_format::save_teab(&document)
        .map_err(|err| format!("failed to encode {} as .teab: {err}", input.display()))?;

    let output_path = output.map(Path::to_path_buf).unwrap_or_else(|| input.with_extension("teab"));
    std::fs::write(&output_path, &teab_bytes)
        .map_err(|err| format!("failed to write {}: {err}", output_path.display()))?;

    println!(
        "compiled {} -> {} ({} bytes)",
        input.display(),
        output_path.display(),
        teab_bytes.len()
    );
    Ok(())
}

/// Writes `document` to `path`, encoding as `.teab` unless `path` itself
/// ends in `.tea` — used by `optimize`, which (unlike `compile`, always
/// `.teab`) lets the caller pick either output format via the output path's
/// own extension.
fn save_document_by_extension(document: &Document, path: &Path) -> Result<usize, String> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("tea") {
        let json = an1mbyt3_format::save_tea(document).map_err(|err| format!("failed to encode {} as .tea: {err}", path.display()))?;
        std::fs::write(path, &json).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        Ok(json.len())
    } else {
        let bytes = an1mbyt3_format::save_teab(document).map_err(|err| format!("failed to encode {} as .teab: {err}", path.display()))?;
        std::fs::write(path, &bytes).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        Ok(bytes.len())
    }
}

fn optimize(input: &Path, output: Option<&Path>, tolerance: f32) -> Result<(), String> {
    let mut document = load_document_by_extension(input)?;
    let stats = optimize::optimize_document(&mut document, tolerance);

    let output_path = output.map(Path::to_path_buf).unwrap_or_else(|| input.to_path_buf());
    let bytes_written = save_document_by_extension(&document, &output_path)?;

    println!(
        "optimized {} -> {} ({bytes_written} bytes, {} keyframe(s) removed)",
        input.display(),
        output_path.display(),
        stats.keyframes_removed
    );
    Ok(())
}

fn validate(input: &Path) -> Result<(), String> {
    let document = load_document_by_extension(input)?;

    println!("{} is a valid document (schema version {})", input.display(), an1mbyt3_format::CURRENT_SCHEMA_VERSION);
    for (_, artboard) in document.artboards() {
        println!(
            "  artboard \"{}\" ({}x{}): {} root node(s), {} shape(s)",
            artboard.name,
            artboard.width,
            artboard.height,
            artboard.root_nodes().len(),
            artboard.shapes().count(),
        );
    }
    Ok(())
}
