#!/usr/bin/env node
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { fixtures } from '../evaluation/fixtures.mjs';
import { exhaustiveOracle, oracleProblem } from '../evaluation/oracle.mjs';
import { artifactReport, hash } from '../evaluation/artifacts.mjs';
const run = promisify(execFile);
const options = Object.fromEntries(process.argv.slice(2).map(argument => { const [key, ...value] = argument.replace(/^--/, '').split('='); return [key, value.join('=') || 'true']; }));
if (!options.bin) throw Error('Provide --bin=/absolute/path/to/compiler.');
for (const name of Object.keys(options)) if (!['bin', 'output'].includes(name)) throw Error(`Unknown argument --${name}`);
const output = resolve(options.output || join(tmpdir(), `lightningcss-compact-oracle-${Date.now()}`));
await mkdir(output, { recursive: true });
const binary = await readFile(resolve(options.bin));
const executable = join(output, 'compiler.snapshot');
await writeFile(executable, binary, { mode: 0o755 });
const report = { status: 'running', compiler: { sha256: hash(binary), sourcePath: resolve(options.bin), executable }, cases: [], cost: { brotliQuality: 5, brotliWindow: 22, gzipLevel: 9, boundaries: 'One independent stylesheet and one independent HTML artifact per oracle fixture.' } };
try {
    for (const [id, propertyCount] of [['shared-single', 1], ['shared-bundle', 2]]) {
        const fixture = fixtures().find(candidate => candidate.id === id);
        const oracle = exhaustiveOracle(oracleProblem(propertyCount));
        const directory = join(output, id);
        await mkdir(directory, { recursive: true });
        await writeFile(join(directory, 'project.json'), JSON.stringify(fixture.input, null, 2));
        await run(executable, ['--project', join(directory, 'project.json'), '--out-dir', directory, '--mode', 'compact'], { timeout: 60000, maxBuffer: 16 * 1024 * 1024 });
        const compiled = JSON.parse(await readFile(join(directory, 'result.json'), 'utf8'));
        const measured = artifactReport(compiled.stylesheets, compiled.bindings);
        const entry = { id, oracle, candidate: { stylesheets: compiled.stylesheets, bindings: compiled.bindings, measurement: measured } };
        report.cases.push(entry);
        assert.equal(measured.total.brotli, oracle.best.measurement.total.brotli, `${id}: compiler must match the independent finite-domain Brotli optimum`);
        assert.equal(measured.total.gzip, oracle.best.measurement.total.gzip, `${id}: equal Brotli costs must use the finite-domain gzip tie break`);
        assert.equal(measured.total.raw, oracle.best.measurement.total.raw, `${id}: equal compressed costs must use the finite-domain raw-byte tie break`);
        console.log(`${id}: ${oracle.leafCount} exact leaves; compiler matches ${measured.total.brotli}/${measured.total.gzip}/${measured.total.raw} byte optimum`);
    }
    report.status = 'passed';
} catch (error) { report.status = 'failed'; report.error = error.stack; process.exitCode = 1; console.error(error.message); }
await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2));
console.log(`Raw oracle report: ${join(output, 'report.json')}`);
