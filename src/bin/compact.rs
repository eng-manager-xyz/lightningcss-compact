use clap::{Parser, Subcommand};
use lightningcss_compact::{compile_project, CompiledProject, Error, Mode, Options, ProjectInput};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;

#[derive(Parser)]
#[command(
    version,
    about = "Cascade-preserving CSS and class compression using Lightning CSS"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    #[arg(long, global = true)]
    out_dir: Option<PathBuf>,
    #[arg(long, global = true, default_value = "compact", value_parser = ["baseline", "naming", "compact"])]
    mode: String,
}
#[derive(Subcommand)]
enum Command {
    /// Report opportunities and encoded artifact sizes without rewriting inputs.
    Analyze,
    /// Emit CSS, bound literals, manifest, maps and report into a separate output directory.
    Build,
    /// Validate the project and assert non-regression against ordinary Lightning CSS.
    Check,
    /// Compare baseline, naming-only and complete optimization using identical encoder settings.
    Eval,
}

fn validate_target(output: &Path, path: &Path, source: &Path) -> Result<(), Error> {
    if path == source || same_file::is_same_file(path, source).unwrap_or(false) {
        return Err(Error::Inventory(
            "output would overwrite the project input".into(),
        ));
    }
    let mut ancestor = path.to_path_buf();
    while ancestor != output {
        if fs::symlink_metadata(&ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(Error::Inventory(format!(
                "output path contains symlink: {}",
                ancestor.display()
            )));
        }
        if !ancestor.pop() {
            return Err(Error::Inventory("output escaped its directory".into()));
        }
    }
    Ok(())
}

fn emit(directory: &Path, project: &CompiledProject, source: &Path) -> Result<(), Error> {
    fs::create_dir_all(directory)?;
    let source = fs::canonicalize(source)?;
    let output = fs::canonicalize(directory)?;
    let mut files = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for (id, css) in &project.stylesheets {
        if id.contains(['\\', ':'])
            || id.chars().any(|character| {
                character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
            })
            || !id.ends_with(".css")
            || id.split('/').any(|part| part.is_empty() || part == ".")
            || Path::new(id)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::Inventory(format!(
                "unsafe stylesheet output identity {id:?}; use a relative .css path"
            )));
        }
        for component in id.split('/') {
            let base = component
                .split('.')
                .next()
                .unwrap_or(component)
                .trim_end_matches(' ')
                .to_ascii_lowercase();
            if component.ends_with(['.', ' '])
                || matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
                || (base.len() == 4
                    && (base.starts_with("com") || base.starts_with("lpt"))
                    && matches!(base.as_bytes()[3], b'1'..=b'9'))
            {
                return Err(Error::Inventory(format!(
                    "nonportable stylesheet output identity {id:?}"
                )));
            }
        }
        let portable: String = id.nfd().flat_map(char::to_lowercase).collect();
        if !identities.insert(portable) {
            return Err(Error::Inventory(format!("stylesheet output identities collide on a case-insensitive or Unicode-normalizing filesystem: {id}")));
        }
        files.push((output.join("css").join(id), css.as_bytes().to_vec()));
    }
    for (name, value) in [
        ("result.json", serde_json::to_vec_pretty(project)?),
        (
            "manifest.json",
            serde_json::to_vec_pretty(&project.manifest)?,
        ),
        ("report.json", serde_json::to_vec_pretty(&project.report)?),
        (
            "bindings.json",
            serde_json::to_vec_pretty(&project.bindings)?,
        ),
    ] {
        files.push((output.join(name), value));
    }
    for (id, map) in &project.source_maps {
        files.push((
            output.join("css").join(format!("{id}.map")),
            map.as_bytes().to_vec(),
        ));
    }
    // Validate every target before writing any artifact. Never follow a source
    // alias or a symlink outside the declared output directory.
    let destination_key = |path: &Path| -> String {
        path.strip_prefix(&output)
            .expect("generated destination is inside its output directory")
            .to_string_lossy()
            .nfd()
            .flat_map(char::to_lowercase)
            .collect()
    };
    let mut destinations = std::collections::BTreeSet::new();
    for (path, _) in &files {
        if !destinations.insert(destination_key(path)) {
            return Err(Error::Inventory(format!(
                "output destinations collide: {}",
                path.display()
            )));
        }
    }
    for (path, _) in &files {
        for ancestor in path
            .ancestors()
            .skip(1)
            .take_while(|ancestor| *ancestor != output.as_path())
        {
            if destinations.contains(&destination_key(ancestor)) {
                return Err(Error::Inventory(format!(
                    "output destination conflicts with a planned file: {}",
                    path.display()
                )));
            }
        }
        validate_target(&output, path, &source)?;
    }
    for (path, bytes) in files {
        fs::create_dir_all(path.parent().expect("output parent"))?;
        fs::write(path, bytes)?;
    }
    Ok(())
}

fn run(cli: Cli) -> Result<(), Error> {
    let path = cli.project.ok_or_else(|| {
        Error::Inventory("--project is required; use --help for the inventory contract".into())
    })?;
    let input: ProjectInput = serde_json::from_slice(&fs::read(&path)?)?;
    let mode = match cli.mode.as_str() {
        "baseline" => Mode::Baseline,
        "naming" => Mode::Naming,
        _ => Mode::Compact,
    };
    if matches!(cli.command, Some(Command::Eval)) {
        let out = cli
            .out_dir
            .ok_or_else(|| Error::Inventory("eval requires --out-dir".into()))?;
        fs::create_dir_all(&out)?;
        let out = fs::canonicalize(out)?;
        let comparison = out.join("comparison.json");
        validate_target(&out, &comparison, &fs::canonicalize(&path)?)?;
        let mut reports = std::collections::BTreeMap::new();
        for (label, mode) in [
            ("baseline", Mode::Baseline),
            ("naming", Mode::Naming),
            ("compact", Mode::Compact),
        ] {
            let compiled = compile_project(
                input.clone(),
                Options {
                    mode,
                    ..Default::default()
                },
            )?;
            emit(&out.join(label), &compiled, &path)?;
            reports.insert(label, compiled.report);
        }
        fs::write(comparison, serde_json::to_vec_pretty(&reports)?)?;
        println!("{}", serde_json::to_string_pretty(&reports)?);
        return Ok(());
    }
    let compiled = compile_project(
        input,
        Options {
            mode,
            ..Default::default()
        },
    )?;
    match cli.command.unwrap_or(Command::Build) {
        Command::Build => emit(
            &cli.out_dir
                .ok_or_else(|| Error::Inventory("build requires --out-dir".into()))?,
            &compiled,
            &path,
        )?,
        Command::Check => {
            if compiled.report.optimized.total.brotli > compiled.report.baseline.total.brotli
                || compiled.report.optimized.total.gzip > compiled.report.baseline.total.gzip
            {
                return Err(Error::Inventory(
                    "encoded artifacts regress against the baseline".into(),
                ));
            }
        }
        Command::Analyze => {}
        Command::Eval => unreachable!(),
    }
    println!("{}", serde_json::to_string_pretty(&compiled.report)?);
    Ok(())
}
fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
