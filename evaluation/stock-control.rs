//! Independent control: unmodified upstream Lightning CSS alpha.72 defaults.
//! This deliberately does not call compact preparation, visitors, or guards.
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss_compact::ProjectInput;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let project = args
        .next()
        .ok_or("usage: stock-control project.json result.json")?;
    let output = args
        .next()
        .ok_or("usage: stock-control project.json result.json")?;
    if args.next().is_some() {
        return Err("usage: stock-control project.json result.json".into());
    }
    let input: ProjectInput = serde_json::from_slice(&fs::read(project)?)?;
    let mut stylesheets = BTreeMap::new();
    for sheet in &input.stylesheets {
        let mut parsed = StyleSheet::parse(
            &sheet.source,
            ParserOptions {
                filename: sheet.id.clone(),
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())?;
        parsed
            .minify(MinifyOptions::default())
            .map_err(|error| error.to_string())?;
        let printed = parsed.to_css(PrinterOptions {
            minify: true,
            ..Default::default()
        })?;
        stylesheets.insert(sheet.id.clone(), printed.code);
    }
    let bindings: BTreeMap<_, _> = input
        .bindings
        .iter()
        .map(|binding| (&binding.id, &binding.value))
        .collect();
    let identities: BTreeMap<_, _> = input
        .managed_classes
        .iter()
        .map(|name| (name, name))
        .collect();
    let result = serde_json::json!({
        "stylesheets": stylesheets,
        "bindings": bindings,
        "manifest": { "identities": identities },
        "control": "stock Lightning CSS alpha.72 parse/minify/print defaults; no compact visitors or guards",
    });
    // An evidence helper must never overwrite an input or an existing result,
    // including a symlink alias. Its caller provides a fresh output location.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&result)?)?;
    Ok(())
}
