#!/usr/bin/env node
// Freeze readable summaries and original synthetic evidence without binaries.
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { createGzip } from 'node:zlib';

const options = {}, counterexamples = [];
for (const argument of process.argv.slice(2)) {
    const [name, ...parts] = argument.replace(/^--/, '').split('=');
    if (name === 'counterexample') {
        const label = parts.shift(), directory = parts.join('=');
        if (!/^[a-z0-9-]+$/.test(label) || !directory) throw Error('Use --counterexample=label=/absolute/directory.');
        counterexamples.push({ label, directory: resolve(directory) });
    } else options[name] = parts.join('=');
}
const allowed = new Set(['native', 'timings', 'oracle', 'output', 'archive']);
for (const name of Object.keys(options)) if (!allowed.has(name)) throw Error(`Unknown argument --${name}`);
for (const name of ['native', 'oracle', 'output', 'archive']) if (!options[name]) throw Error(`Missing --${name}`);
const nativeDir = resolve(options.native), oracleDir = resolve(options.oracle), output = resolve(options.output), archive = resolve(options.archive);
const native = JSON.parse(await readFile(join(nativeDir, 'report.json'), 'utf8'));
const oracle = JSON.parse(await readFile(join(oracleDir, 'report.json'), 'utf8'));
const timingDir = options.timings ? resolve(options.timings) : null;
const timing = timingDir ? JSON.parse(await readFile(join(timingDir, 'report.json'), 'utf8')) : null;
if (native.status !== 'passed' || !native.fixtures.length || native.exceptions.length || native.unexpectedRequests.length) throw Error('Native report did not pass.');
if (oracle.status !== 'passed' || !oracle.cases.length || oracle.compiler?.sha256 !== native.compiler.sha256) throw Error('Oracle report must pass against the same frozen compiler.');
for (const fixture of native.fixtures) for (const state of fixture.states) {
    if (fixture.status !== 'passed' || !fixture.deterministic || !state.sourceEqual || !state.sourceScreenshotEqual || !state.equal || !state.screenshotEqual || !state.mutant.detected) throw Error(`Unverified native state ${fixture.id}/${state.state}.`);
}
const timedFixture = timing?.fixtures.find(fixture => fixture.id === 'timing-many-nodes');
if (timing && (timing.status !== 'passed' || !timedFixture?.timings.length || timing.exceptions.length || timing.unexpectedRequests.length || timing.compiler.sha256 !== native.compiler.sha256 || timing.protocol.cpuRate !== native.protocol.cpuRate || timing.browser.product !== native.browser.product || JSON.stringify(timing.host) !== JSON.stringify(native.host) || JSON.stringify(timing.gpu) !== JSON.stringify(native.gpu))) throw Error('Timing report must pass on the same host, frozen compiler, CPU shaping, Chrome and GPU.');
const modes = ['stockControl', 'baseline', 'naming', 'compact'].filter(mode => native.fixtures.every(fixture => fixture.size[mode]));
const totals = Object.fromEntries(modes.map(mode => [mode, Object.fromEntries(['raw', 'brotli', 'gzip'].map(codec => [codec, native.fixtures.reduce((sum, fixture) => sum + fixture.size[mode].total[codec], 0)]))]));
const reduction = control => Object.fromEntries(['raw', 'brotli', 'gzip'].map(codec => [codec, 100 * (1 - totals.compact[codec] / totals[control][codec])]));
const stockDeviationCount = native.fixtures.reduce((sum, fixture) => sum + fixture.states.filter(state => state.stockControl && (!state.stockControl.equal || !state.stockControl.screenshotEqual)).length, 0);
const summary = {
    schemaVersion: 1, date: native.startedAt.slice(0, 10), status: 'passed',
    compiler: { sha256: native.compiler.sha256 }, stockCompiler: native.stockCompiler ? { sha256: native.stockCompiler.sha256 } : null,
    hardware: native.host, chrome: native.browser, gpu: native.gpu, protocol: native.protocol,
    fixtures: native.fixtures.map(fixture => ({ id: fixture.id, states: fixture.states.length, sizes: Object.fromEntries(modes.map(mode => [mode, fixture.size[mode].total])), stockDeviations: fixture.states.filter(state => state.stockControl && (!state.stockControl.equal || !state.stockControl.screenshotEqual)).map(state => ({ viewport: state.viewport.id, state: state.state, differences: state.stockControl.differences })) })),
    totals, stockDeviationCount,
    reductions: Object.fromEntries(['baseline', 'stockControl'].filter(mode => totals[mode] && (mode !== 'stockControl' || stockDeviationCount === 0)).map(mode => [mode, reduction(mode)])),
    stateCount: native.fixtures.reduce((sum, fixture) => sum + fixture.states.length, 0),
    mutantCount: native.fixtures.reduce((sum, fixture) => sum + fixture.states.filter(state => state.mutant.detected).length, 0),
    exceptions: native.exceptions.length, remoteRequests: native.unexpectedRequests.length,
    oracle,
    timings: timedFixture ? { pairs: timedFixture.timings.length, summary: timedFixture.timingSummary, samples: timedFixture.timings, conclusion: 'Quiet-host paired synthetic measurements are mixed and do not establish a rendering-speed improvement. Paint is measured from actual renderer-main Paint/X trace spans, not inferred from task duration.' } : null,
    evidence: { archive: basename(archive), excluded: ['Derived compiler executables', 'Duplicate per-state observations already contained in original report.json'], rawReports: ['native/report.json', ...timing ? ['timings/report.json'] : [], 'oracle/report.json'] },
};
await mkdir(output, { recursive: true });
await mkdir(resolve(archive, '..'), { recursive: true });
const files = [];
async function addDirectory(directory, prefix, accept) {
    async function walk(relative = '') {
        for (const entry of (await readdir(join(directory, relative), { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
            const path = join(relative, entry.name);
            if (entry.isDirectory()) await walk(path);
            else if (entry.isFile() && accept(path.replaceAll('\\', '/'))) files.push({ source: join(directory, path), name: `${prefix}/${path.replaceAll('\\', '/')}` });
        }
    }
    await walk();
}
const evidenceFile = path => path === 'report.json' || path.endsWith('.png') || path.endsWith('.trace.json.gz') || (path.startsWith('compiler/') && /\/(project|result|manifest|report)\.json$/.test(path));
await addDirectory(nativeDir, 'native', evidenceFile);
if (timingDir) await addDirectory(timingDir, 'timings', evidenceFile);
await addDirectory(oracleDir, 'oracle', path => path.endsWith('.json'));
for (const counter of counterexamples) await addDirectory(counter.directory, `counterexamples/${counter.label}`, path => !path.includes('snapshot') && /\.(json|png|css|mjs|txt|log)$/.test(path));
files.sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
for (let index = 0; index < files.length; index++) {
    if (index && files[index - 1].name === files[index].name) throw Error(`Duplicate evidence path: ${files[index].name}`);
    if (resolve(files[index].source) === archive) throw Error('Evidence archive must not overwrite an input artifact.');
}
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const manifest = [];
for (const file of files) { const bytes = await readFile(file.source); manifest.push({ path: file.name, bytes: bytes.length, sha256: sha(bytes) }); }
const expectedFiles = new Map(manifest.map(file => [file.path, file]));
summary.evidence.fileCount = manifest.length;
summary.evidence.uncompressedBytes = manifest.reduce((sum, file) => sum + file.bytes, 0);
const manifestBytes = Buffer.from(JSON.stringify({ schemaVersion: 1, compiler: summary.compiler, stockCompiler: summary.stockCompiler, files: manifest }, null, 2) + '\n');
await writeFile(join(output, 'evidence-manifest.json'), manifestBytes);

function tarHeader(name, size) {
    const header = Buffer.alloc(512), parts = name.split('/');
    let tail = name, prefix = '';
    while (Buffer.byteLength(tail) > 100 && parts.length > 1) { prefix += `${prefix ? '/' : ''}${parts.shift()}`; tail = parts.join('/'); }
    if (Buffer.byteLength(tail) > 100 || Buffer.byteLength(prefix) > 155) throw Error(`Tar evidence path too long: ${name}`);
    header.write(tail, 0, 100); header.write(prefix, 345, 155);
    const octal = (offset, width, value) => header.write(value.toString(8).padStart(width - 1, '0') + '\0', offset, width);
    octal(100, 8, 0o644); octal(108, 8, 0); octal(116, 8, 0); octal(124, 12, size); octal(136, 12, 0);
    header.fill(32, 148, 156); header[156] = 48; header.write('ustar\0', 257, 6); header.write('00', 263, 2);
    const checksum = header.reduce((sum, byte) => sum + byte, 0);
    header.write(checksum.toString(8).padStart(6, '0') + '\0 ', 148, 8);
    return header;
}
async function* tarContents() {
    for (const file of [...files, { name: 'evidence-manifest.json', data: manifestBytes }]) {
        const bytes = file.data || await readFile(file.source);
        const expected = expectedFiles.get(file.name);
        if (expected && (expected.bytes !== bytes.length || expected.sha256 !== sha(bytes))) throw Error(`Evidence changed while archiving: ${file.name}`);
        yield tarHeader(file.name, bytes.length); yield bytes;
        if (bytes.length % 512) yield Buffer.alloc(512 - bytes.length % 512);
    }
    yield Buffer.alloc(1024);
}
await pipeline(Readable.from(tarContents()), createGzip({ level: 9 }), createWriteStream(archive));
const archiveBytes = await readFile(archive);
summary.evidence.sha256 = sha(archiveBytes);
summary.evidence.archiveBytes = archiveBytes.length;
await writeFile(join(output, 'evidence.sha256'), `${summary.evidence.sha256}  ${basename(archive)}\n`);
await writeFile(join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
console.log(`Summary: ${join(output, 'summary.json')}`);
console.log(`Evidence: ${archive} (${(await stat(archive)).size} bytes, SHA256 ${summary.evidence.sha256})`);
