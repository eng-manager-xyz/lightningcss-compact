use crate::{ArtifactSizes, BindingKind, EncodedSize};
use std::{collections::BTreeMap, io::Write};

/// The evaluator uses exactly these encoder settings. Artifacts are independent streams.
pub fn encoded_size(bytes: &[u8]) -> Result<EncodedSize, crate::Error> {
    let mut brotli_bytes = Vec::new();
    {
        let mut encoder = brotli::CompressorWriter::new(&mut brotli_bytes, 4096, 5, 22);
        encoder.write_all(bytes)?;
    }
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gzip.write_all(bytes)?;
    Ok(EncodedSize {
        raw: bytes.len(),
        brotli: brotli_bytes.len(),
        gzip: gzip.finish()?.len(),
    })
}

fn add(to: &mut EncodedSize, from: &EncodedSize) {
    to.raw += from.raw;
    to.brotli += from.brotli;
    to.gzip += from.gzip;
}

pub(crate) fn measure(
    css: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, String>,
    kinds: &BTreeMap<String, BindingKind>,
) -> Result<ArtifactSizes, crate::Error> {
    let mut sizes = ArtifactSizes::default();
    for value in css.values() {
        add(&mut sizes.css, &encoded_size(value.as_bytes())?);
    }
    // Preserve complete HTML corpus streams. Small class/selector bindings are a
    // separate deterministic synthetic JS/template stream, not invented network requests.
    let mut literals = String::new();
    for (id, value) in bindings {
        if kinds[id] == BindingKind::Html {
            add(&mut sizes.html, &encoded_size(value.as_bytes())?);
        } else {
            literals.push_str(&serde_json::to_string(value)?);
            literals.push(';');
        }
    }
    if !literals.is_empty() {
        sizes.javascript = encoded_size(literals.as_bytes())?;
    }
    add(&mut sizes.total, &sizes.css);
    add(&mut sizes.total, &sizes.html);
    add(&mut sizes.total, &sizes.javascript);
    Ok(sizes)
}
