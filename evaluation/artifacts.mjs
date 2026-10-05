import { createHash } from 'node:crypto';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

export const hash = value => createHash('sha256').update(value).digest('hex');
export function artifactSize(value) {
    const bytes = Buffer.from(value);
    return {
        raw: bytes.length,
        brotli: brotliCompressSync(bytes, { params: { [constants.BROTLI_PARAM_QUALITY]: 5 } }).length,
        gzip: gzipSync(bytes, { level: 9, mtime: 0 }).length,
        sha256: hash(bytes),
    };
}
export function artifactReport(stylesheets, bindings, bindingKinds = {}) {
    const artifacts = {};
    for (const [id, css] of Object.entries(stylesheets)) artifacts[`css:${id}`] = artifactSize(css);
    let literals = '';
    for (const [id, value] of Object.entries(bindings).sort(([left], [right]) => Buffer.compare(Buffer.from(left), Buffer.from(right)))) {
        if ((bindingKinds[id] || 'html') === 'html') artifacts[`html:${id}`] = artifactSize(value);
        else literals += `${JSON.stringify(value)};`;
    }
    if (literals) artifacts['javascript:binding-literals'] = artifactSize(literals);
    return {
        protocol: { brotliQuality: 5, gzipLevel: 9, boundaries: 'Each stylesheet and full HTML binding compressed independently; non-HTML literals form one sorted JSON-literal synthetic JS/template stream.' },
        artifacts,
        total: Object.values(artifacts).reduce((sum, value) => ({ raw: sum.raw + value.raw, brotli: sum.brotli + value.brotli, gzip: sum.gzip + value.gzip }), { raw: 0, brotli: 0, gzip: 0 }),
    };
}
export function differences(left, right, path = '') {
    if (Object.is(left, right)) return [];
    if (!left || !right || typeof left !== 'object' || typeof right !== 'object') return [{ path, before: left, after: right }];
    return [...new Set([...Object.keys(left), ...Object.keys(right)])].flatMap(key => differences(left[key], right[key], path ? `${path}.${key}` : key));
}
export function summary(values) {
    const ordered = [...values].sort((a, b) => a - b);
    const pick = fraction => ordered[Math.min(ordered.length - 1, Math.floor(fraction * ordered.length))];
    const median = (ordered[Math.floor((ordered.length - 1) / 2)] + ordered[Math.floor(ordered.length / 2)]) / 2;
    return ordered.length ? { count: ordered.length, min: ordered[0], p10: pick(.1), median, p90: pick(.9), max: ordered.at(-1) } : null;
}
