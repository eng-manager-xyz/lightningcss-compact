#!/usr/bin/env node
// Standalone synthetic evaluator. It renders real CSS in native Chrome;
// fixtures and deliberate mutants are independent of optimizer internals.
import { execFile, spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir, platform, cpus } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { gzipSync } from 'node:zlib';
import { Cdp } from './local-chrome-cdp.mjs';
import { fixtures } from '../evaluation/fixtures.mjs';
import { artifactReport, differences, hash, summary } from '../evaluation/artifacts.mjs';
import { paintSummary } from '../evaluation/paint.mjs';

const run = promisify(execFile);
const options = Object.fromEntries(process.argv.slice(2).map(argument => {
    const [key, ...value] = argument.replace(/^--/, '').split('=');
    return [key, value.join('=') || 'true'];
}));
const allowedOptions = new Set(['bin', 'stock-bin', 'output', 'chrome', 'self-test', 'fixture', 'timing-fixture', 'timing-pairs', 'cpu-rate']);
for (const name of Object.keys(options)) if (!allowedOptions.has(name)) throw Error(`Unknown argument: --${name}`);
const selfTest = options['self-test'] === 'true';
if (!selfTest && !options.bin) throw Error('Provide --bin=/absolute/path/to/compiler or use --self-test for harness validation.');
if (selfTest && options['stock-bin']) throw Error('The independent stock control requires an actual compiler evaluation.');
const output = resolve(options.output || join(tmpdir(), `lightningcss-compact-evaluation-${Date.now()}`));
const timingPairs = Number(options['timing-pairs'] ?? (selfTest ? 0 : 8));
const cpuRate = Number(options['cpu-rate'] ?? 4);
const timingFixtures = (options['timing-fixture'] || 'timing-many-nodes').split(',');
if (!Number.isInteger(timingPairs) || timingPairs < 0 || timingPairs > 50 || !Number.isFinite(cpuRate) || cpuRate < 1 || cpuRate > 20) throw Error('Invalid timing-pairs or cpu-rate.');
const selected = fixtures().filter(fixture => !options.fixture || options.fixture.split(',').includes(fixture.id));
if (!selected.length) throw Error('No matching fixtures.');
await mkdir(output, { recursive: true });
// Freeze the actual executable. A concurrent rebuild must not mix compiler
// versions across baseline/candidate fixtures or repeated-output assertions.
let compilerExecutable = null, compilerHash = null;
let stockExecutable = null, stockHash = null;
if (!selfTest) {
    const binary = await readFile(resolve(options.bin));
    compilerHash = hash(binary);
    compilerExecutable = join(output, platform() === 'win32' ? 'compiler.snapshot.exe' : 'compiler.snapshot');
    await writeFile(compilerExecutable, binary, { mode: 0o755 });
}
if (options['stock-bin']) {
    const binary = await readFile(resolve(options['stock-bin']));
    stockHash = hash(binary);
    stockExecutable = join(output, platform() === 'win32' ? 'stock-control.snapshot.exe' : 'stock-control.snapshot');
    await writeFile(stockExecutable, binary, { mode: 0o755 });
}
const profile = await mkdtemp(join(tmpdir(), 'lightningcss-compact-chrome-'));
const documents = new Map();
const delay = milliseconds => new Promise(resolveDelay => setTimeout(resolveDelay, milliseconds));
const report = {
    schemaVersion: 1, startedAt: new Date().toISOString(), status: 'running',
    host: { platform: platform(), cpu: cpus()[0]?.model, logicalCpus: cpus().length, node: process.version },
    compiler: selfTest ? null : { sourcePath: resolve(options.bin), executable: compilerExecutable, sha256: compilerHash },
    stockCompiler: stockExecutable ? { sourcePath: resolve(options['stock-bin']), executable: stockExecutable, sha256: stockHash } : null,
    protocol: {
        nativeChrome: true, selfTest, cpuRate, timingPairs, timingFixtures,
        readiness: 'Scripting-enabled: native FontFaceSet readiness and six rendered frames with unchanged ResizeObserver dimensions, then two paint opportunities. Scripting-disabled: loaded native FontFaceSet and six unchanged layout samples after native screenshot presentations; page callbacks remain disabled.',
        comparison: 'Authored source → guarded Lightning CSS baseline → compact candidate: all computed properties and geometry of stable data-probe nodes, generated ::before/::after styles, query-selector matches, and byte-identical native viewport PNGs.',
        timings: 'Fresh-document AB/BA alternation in focused timing fixtures; native CDP Style/Layout/Task metrics and actual trace Paint/X renderer-main intervals. Full gzip traces retained. Informational synthetic measurements, not a rendering-speed or phone certification.',
        externalRequests: 'No fixture depends on remote styles, scripts, images, or fonts.',
        stockControl: stockExecutable ? 'Independent stock alpha.72 parse/minify(default)/print helper. Source deviations are recorded separately; this control is never the correctness truth or the guarded timing baseline.' : null,
    }, fixtures: [], exceptions: [], unexpectedRequests: [], navigationReadRetries: [],
};
const server = createServer((request, response) => {
    const document = documents.get(new URL(request.url, 'http://localhost').pathname);
    if (!document) { response.writeHead(404).end(); return; }
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
    response.end(document);
});
await new Promise(resolveListen => server.listen(0, '127.0.0.1', resolveListen));
const origin = `http://127.0.0.1:${server.address().port}`;
let child, browser, page, stderr = '';

function identity(fixture) {
    return {
        stylesheets: Object.fromEntries(fixture.input.stylesheets.map(sheet => [sheet.id, sheet.source])),
        bindings: Object.fromEntries(fixture.input.bindings.map(binding => [binding.id, binding.value])),
        manifest: { identities: Object.fromEntries(fixture.input.managed_classes.map(name => [name, name])) },
    };
}
async function compile(fixture, mode, suffix = '') {
    const directory = join(output, 'compiler', `${fixture.id}-${mode}${suffix}`);
    await mkdir(directory, { recursive: true });
    const project = join(directory, 'project.json');
    await writeFile(project, JSON.stringify(fixture.input, null, 2));
    const result = await run(compilerExecutable, ['--project', project, '--out-dir', directory, '--mode', mode], { maxBuffer: 16 * 1024 * 1024, timeout: 60000 });
    await writeFile(join(directory, 'stdout.txt'), result.stdout);
    await writeFile(join(directory, 'stderr.txt'), result.stderr);
    const compiled = JSON.parse(await readFile(join(directory, 'result.json'), 'utf8'));
    if (!compiled.stylesheets || !compiled.bindings?.document || !compiled.manifest?.identities) throw Error(`Invalid CLI result for ${fixture.id}/${mode}`);
    return compiled;
}
async function compileStock(fixture) {
    const directory = join(output, 'compiler', `${fixture.id}-stock-control`);
    await mkdir(directory, { recursive: true });
    const project = join(directory, 'project.json'), result = join(directory, 'result.json');
    await writeFile(project, JSON.stringify(fixture.input, null, 2));
    const execution = await run(stockExecutable, [project, result], { maxBuffer: 16 * 1024 * 1024, timeout: 60000 });
    await writeFile(join(directory, 'stdout.txt'), execution.stdout);
    await writeFile(join(directory, 'stderr.txt'), execution.stderr);
    const compiled = JSON.parse(await readFile(result, 'utf8'));
    if (!compiled.stylesheets || !compiled.bindings?.document || !compiled.manifest?.identities) throw Error(`Invalid independent stock-control result for ${fixture.id}`);
    return compiled;
}
function register(fixture, mode, compiled, mutation = '') {
    const route = `/${fixture.id}/${mode}`;
    // Chrome's inherited tap-highlight UA default can switch after mobile
    // emulation, and its overlay scrollbar fades independently of DOM/font
    // readiness. Author the same reset on every document rather than dropping
    // computed properties or masking screenshot pixels from the assertions.
    documents.set(route, `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"><style>html{scrollbar-width:none}::-webkit-scrollbar{display:none}body{margin:16px;font:16px Arial,sans-serif;color:black;-webkit-tap-highlight-color:rgba(0,0,0,.18)}main{max-width:800px}button{font:inherit}</style><style>${Object.values(compiled.stylesheets).join('\n')}</style>${mutation ? `<style>${mutation}</style>` : ''}</head><body>${compiled.bindings.document}</body></html>`);
    return route;
}
async function evaluate(expression) {
    const result = await page.command('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    return result.result.value;
}
async function until(expression, label, deadline = 12000) {
    const end = Date.now() + deadline;
    while (Date.now() < end) {
        try {
            if (await evaluate(expression)) return;
        } catch (error) {
            // Page.navigate may retire the old execution context before this
            // specific document-read command reaches Chrome. Retry only that
            // protocol condition within this existing navigation deadline.
            if (error.cdp?.code !== -32000 || !/Execution context was destroyed|Cannot find context with specified id/.test(error.cdp.message)) throw error;
            report.navigationReadRetries.push({ label, message: error.cdp.message });
        }
        await delay(25);
    }
    throw Error(`Timed out: ${label}`);
}
async function ready(scriptDisabled = false) {
    if (scriptDisabled) {
        await until("document.fonts.status==='loaded'", 'scripting-disabled native fonts');
        const samples = [];
        let previous = null, stable = 0;
        const end = Date.now() + 12000;
        while (Date.now() < end) {
            // Page callbacks are intentionally disabled. A native screenshot
            // presents the actual document before synchronously inspecting its
            // layout through DevTools; no page timer/RAF is enabled to render it.
            await page.command('Page.captureScreenshot', { format: 'png', fromSurface: true });
            const dimensions = await evaluate("[document.documentElement,document.querySelector('main'),...document.querySelectorAll('[data-probe]')].filter(Boolean).map(node=>{const r=node.getBoundingClientRect();return{probe:node.dataset.probe||node.tagName,width:r.width,height:r.height,scrollWidth:node.scrollWidth,scrollHeight:node.scrollHeight}})");
            const encoded = JSON.stringify(dimensions);
            stable = encoded === previous ? stable + 1 : 0;
            previous = encoded;
            samples.push(dimensions);
            if (stable >= 6) return { mode: 'scripting-disabled', nativeFonts: 'loaded', presentations: samples.length, dimensions: samples };
        }
        throw Error('Scripting-disabled native layout readiness deadline');
    }
    return evaluate(`(async()=>{
        await document.fonts.ready;
        if(document.fonts.status!=='loaded')throw Error('Fonts remain loading');
        const nodes=[document.documentElement,document.querySelector('main'),...document.querySelectorAll('[data-probe]')].filter(Boolean);
        return new Promise((resolve,reject)=>{
            let epoch=0,last=-1,stable=0,seen=new Set(),events=[],frame;
            const observer=new ResizeObserver(entries=>{epoch++;for(const entry of entries){seen.add(entry.target);events.push({probe:entry.target.dataset.probe||entry.target.tagName,width:entry.contentRect.width,height:entry.contentRect.height});}});
            nodes.forEach(node=>observer.observe(node));
            const finish=error=>{clearTimeout(timer);cancelAnimationFrame(frame);observer.disconnect();document.fonts.removeEventListener('loading',changed);document.fonts.removeEventListener('loadingdone',changed);document.fonts.removeEventListener('loadingerror',changed);if(error)reject(error);else resolve({observedNodes:seen.size,events});};
            const changed=()=>{epoch++;stable=0;};
            for(const name of ['loading','loadingdone','loadingerror'])document.fonts.addEventListener(name,changed);
            const timer=setTimeout(()=>finish(Error('Font/ResizeObserver readiness deadline; '+JSON.stringify(events.slice(-8)))),12000);
            const tick=()=>{if(document.fonts.status==='loaded'&&seen.size===nodes.length){stable=epoch===last?stable+1:0;last=epoch;if(stable>=6){requestAnimationFrame(()=>requestAnimationFrame(()=>finish()));return;}}else stable=0;frame=requestAnimationFrame(tick);};
            frame=requestAnimationFrame(tick);
        });
    })()`);
}
async function action(value, compiled) {
    const probe = value.probe ? `[data-probe=${JSON.stringify(value.probe)}]` : null;
    if (value.type === 'direction') await evaluate(`document.documentElement.dir=${JSON.stringify(value.value)}`);
    else if (value.type === 'writing-mode') await evaluate(`document.documentElement.style.writingMode=${JSON.stringify(value.value)}`);
    else if (value.type === 'media') await page.command('Emulation.setEmulatedMedia', { features: [{ name: value.name, value: value.value }] });
    else if (value.type === 'toggle') {
        const token = value.binding ? compiled.bindings[value.binding] : compiled.manifest.identities[value.logical] || value.logical;
        if (/\s/.test(token)) throw Error('Dynamic token expanded to multiple classes.');
        await evaluate(`document.querySelector(${JSON.stringify(probe)}).classList.toggle(${JSON.stringify(token)},true)`);
    } else if (value.type === 'hover') {
        const point = await evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(probe)}).getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);
        await page.command('Input.dispatchMouseEvent', { type: 'mouseMoved', ...point });
    } else if (value.type === 'focus') await evaluate(`document.querySelector(${JSON.stringify(probe)}).focus()`);
    else throw Error(`Unknown fixture action: ${value.type}`);
}
async function visit(route, viewport, state, compiled) {
    await page.command('Emulation.setScriptExecutionDisabled', { value: state.scriptDisabled || false });
    await page.command('Emulation.setDeviceMetricsOverride', { width: viewport.width, height: viewport.height, deviceScaleFactor: 1, mobile: viewport.id === 'mobile' });
    await page.command('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'no-preference' }] });
    await page.command('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 0, y: 0 });
    const navigation = await page.command('Page.navigate', { url: origin + route });
    if (navigation.errorText) throw Error(`Navigation failed: ${navigation.errorText}`);
    await until(`document.readyState==='complete'&&location.pathname===${JSON.stringify(route)}`, `document ${route}`);
    await documentActions();
    for (const value of state.actions) await action(value, compiled);
    return ready(state.scriptDisabled || false);
    async function documentActions() { await evaluate('document.documentElement.dir="ltr";window.scrollTo(0,0)'); }
}
async function snapshot(compiled, fixture) {
    const selectors = Object.fromEntries(fixture.input.bindings.filter(binding => binding.kind === 'selector').map(binding => [binding.id, compiled.bindings[binding.id]]));
    const observedProperties = [...new Set([
        ...[fixture.expectedStyles, ...fixture.states.map(state => state.expectedStyles || {})].flatMap(styles => Object.values(styles).flatMap(properties => Object.keys(properties))),
        ...Object.values(fixture.expectedPseudoStyles).flatMap(pseudos => Object.values(pseudos).flatMap(properties => Object.keys(properties))),
    ])];
    return evaluate(`(()=>{
        const all=[...document.querySelectorAll('[data-probe]')],sample=all.length<=32?all:all.filter((_,index)=>index%Math.ceil(all.length/30)===0||index===all.length-1);
        const styles=(node,pseudo)=>{const style=getComputedStyle(node,pseudo),out={};for(const property of style)out[property]=style.getPropertyValue(property);for(const property of ${JSON.stringify(observedProperties)})out[property]=style.getPropertyValue(property);return out;};
        return {probeCount:all.length,sampledProbeCount:sample.length,overflow:document.documentElement.scrollWidth>innerWidth,queries:Object.fromEntries(Object.entries(${JSON.stringify(selectors)}).map(([id,selector])=>[id,[...document.querySelectorAll(selector)].map(node=>node.dataset.probe||node.tagName)])),
            probes:Object.fromEntries(sample.map(node=>{const rect=node.getBoundingClientRect();return [node.dataset.probe,{style:styles(node),before:styles(node,'::before'),after:styles(node,'::after'),rect:[rect.x,rect.y,rect.width,rect.height],scroll:[node.scrollWidth,node.scrollHeight],text:node.textContent,focused:document.activeElement===node,hovered:node.matches(':hover')}];}))};
    })()`);
}
async function capture(path) {
    const screenshot = await page.command('Page.captureScreenshot', { format: 'png', fromSurface: true });
    const bytes = Buffer.from(screenshot.data, 'base64');
    await writeFile(path, bytes);
    return hash(bytes);
}
async function timing(route, viewport, compiled, traceName) {
    const complete = new Promise(resolveTrace => {
        const off = page.on('Tracing.tracingComplete', event => { off(); resolveTrace(event); });
    });
    await page.command('Tracing.start', { categories: 'devtools.timeline,blink.user_timing', options: 'record-as-much-as-possible', transferMode: 'ReturnAsStream' });
    let metrics;
    try {
        await visit(route, viewport, { actions: [] }, compiled);
        metrics = Object.fromEntries((await page.command('Performance.getMetrics')).metrics.map(metric => [metric.name, metric.value]));
    } finally { await page.command('Tracing.end'); }
    let timer;
    const finished = await Promise.race([complete, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Native trace completion deadline')), 12000); })]).finally(() => clearTimeout(timer));
    if (!finished.stream) throw Error('Native trace did not return a stream.');
    let source = '';
    try {
        for (;;) {
            const chunk = await page.command('IO.read', { handle: finished.stream, size: 1024 * 1024 });
            source += chunk.base64Encoded ? Buffer.from(chunk.data, 'base64').toString('utf8') : chunk.data;
            if (chunk.eof) break;
        }
    } finally { await page.command('IO.close', { handle: finished.stream }); }
    const trace = JSON.parse(source);
    const compressed = gzipSync(source, { level: 9 });
    await writeFile(join(output, traceName), compressed);
    const paint = paintSummary(trace.traceEvents, metrics.NavigationStart * 1e6, metrics.Timestamp * 1e6);
    return { recalcStyleMs: metrics.RecalcStyleDuration * 1000, layoutMs: metrics.LayoutDuration * 1000, taskMs: metrics.TaskDuration * 1000, paintMs: paint.unionMs, recalcCount: metrics.RecalcStyleCount, layoutCount: metrics.LayoutCount, paint, trace: { file: traceName, sha256: hash(compressed), rawBytes: Buffer.byteLength(source), gzipBytes: compressed.length } };
}

try {
    const chrome = options.chrome || process.env.CHROME_BIN || (platform() === 'darwin' ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'google-chrome');
    child = spawn(chrome, ['--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--window-size=960,844', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
    child.on('error', error => { stderr = `${stderr}\n${error.message}`; });
    child.stderr.on('data', data => { stderr = (stderr + data).slice(-8000); });
    let activePort;
    for (let index = 0; index < 100; index++) {
        try { activePort = await readFile(join(profile, 'DevToolsActivePort'), 'utf8'); break; } catch { await delay(50); }
    }
    if (!activePort) throw Error(`Chrome failed to start: ${stderr}`);
    const [port, browserPath] = activePort.trim().split('\n');
    browser = new Cdp(`ws://127.0.0.1:${port}${browserPath}`);
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    page = new Cdp(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
    await Promise.all([page.command('Page.enable'), page.command('Runtime.enable'), page.command('Network.enable'), page.command('Performance.enable')]);
    await page.command('Emulation.setCPUThrottlingRate', { rate: cpuRate });
    page.on('Runtime.exceptionThrown', event => report.exceptions.push(event.exceptionDetails));
    page.on('Network.requestWillBeSent', event => { if (!event.request.url.startsWith(origin + '/') && !event.request.url.startsWith('data:')) report.unexpectedRequests.push(event.request.url); });
    report.browser = await browser.command('Browser.getVersion');
    report.gpu = (await browser.command('SystemInfo.getInfo')).gpu.devices;
    for (const fixture of selected) {
        const original = identity(fixture);
        const baseline = selfTest ? identity(fixture) : await compile(fixture, 'baseline');
        const naming = selfTest ? baseline : await compile(fixture, 'naming');
        const candidate = selfTest ? identity(fixture) : await compile(fixture, 'compact');
        const stock = stockExecutable ? await compileStock(fixture) : null;
        const kinds = Object.fromEntries(fixture.input.bindings.map(binding => [binding.id, binding.kind]));
        const entry = { id: fixture.id, status: 'running', size: { baseline: artifactReport(baseline.stylesheets, baseline.bindings, kinds), naming: artifactReport(naming.stylesheets, naming.bindings, kinds), compact: artifactReport(candidate.stylesheets, candidate.bindings, kinds) }, states: [], timings: [], deterministic: true };
        report.fixtures.push(entry);
        if (stock) entry.size.stockControl = artifactReport(stock.stylesheets, stock.bindings, kinds);
        entry.capabilities = await evaluate(`Object.fromEntries(${JSON.stringify(fixture.requiredSupport)}.map(value=>[value,CSS.supports(value)]))`);
        for (const [declaration, supported] of Object.entries(entry.capabilities)) if (!supported) throw Error(`Native regression requires supported CSS: ${fixture.id} ${declaration}`);
        if (!selfTest) {
            const repeated = await compile(fixture, 'compact', '-repeat');
            const mismatch = differences(candidate, repeated);
            entry.deterministic = mismatch.length === 0;
            if (mismatch.length) throw Error(`Non-deterministic output: ${fixture.id} ${JSON.stringify(mismatch.slice(0, 3))}`);
            if (!fixture.input.complete_usage && candidate.bindings.document !== fixture.input.bindings.find(binding => binding.id === 'document').value) throw Error('Incomplete inventory changed HTML classes.');
        }
        const routes = { original: register(fixture, 'original', original), baseline: register(fixture, 'baseline', baseline), compact: register(fixture, 'compact', candidate), mutant: register(fixture, 'mutant', baseline, '[data-probe]{color:rgb(1,2,3)!important;min-height:137px!important;inline-size:117px!important}') };
        if (stock) routes.stockControl = register(fixture, 'stock-control', stock);
        for (const viewport of fixture.viewports) for (const state of fixture.states) {
            const stem = `${fixture.id}-${viewport.id}-${state.id}`;
            const originalReady = await visit(routes.original, viewport, state, original);
            const source = await snapshot(original, fixture);
            for (const [probe, properties] of Object.entries({ ...fixture.expectedStyles, ...state.expectedStyles })) for (const [property, value] of Object.entries(properties)) {
                const actual = source.probes[probe]?.style[property];
                if (actual !== value) throw Error(`Authored regression precondition failed: ${stem} ${probe}.${property}; expected ${value}, observed ${actual}`);
            }
            for (const [probe, pseudos] of Object.entries(fixture.expectedPseudoStyles)) for (const [pseudo, properties] of Object.entries(pseudos)) for (const [property, value] of Object.entries(properties)) {
                const actual = source.probes[probe]?.[pseudo]?.[property];
                if (actual !== value) throw Error(`Authored pseudo regression precondition failed: ${stem} ${probe}.${pseudo}.${property}; expected ${value}, observed ${actual}`);
            }
            const sourcePng = await capture(join(output, `${stem}-original.png`));
            const beforeReady = await visit(routes.baseline, viewport, state, baseline);
            const before = await snapshot(baseline, fixture);
            const beforePng = await capture(join(output, `${stem}-baseline.png`));
            const baselineMismatch = differences(source, before);
            if (baselineMismatch.length || sourcePng !== beforePng) throw Error(`Guarded baseline changed authored rendering: ${stem}; ${JSON.stringify(baselineMismatch.slice(0, 4))}; screenshotEqual=${sourcePng === beforePng}`);
            const afterReady = await visit(routes.compact, viewport, state, candidate);
            const after = await snapshot(candidate, fixture);
            const afterPng = await capture(join(output, `${stem}-compact.png`));
            const mismatch = differences(before, after);
            const result = { viewport, state: state.id, sourceEqual: !baselineMismatch.length, sourceScreenshotEqual: sourcePng === beforePng, equal: !mismatch.length, differences: mismatch.slice(0, 30), screenshotEqual: beforePng === afterPng, screenshots: { original: sourcePng, before: beforePng, after: afterPng }, readiness: { original: originalReady, before: beforeReady, after: afterReady }, observations: { original: source, before, after } };
            entry.states.push(result);
            if (mismatch.length || beforePng !== afterPng) throw Error(`Browser difference: ${stem}; ${JSON.stringify(mismatch.slice(0, 4))}; screenshotEqual=${beforePng === afterPng}`);
            if (stock) {
                const stockReady = await visit(routes.stockControl, viewport, state, stock);
                const stockObservation = await snapshot(stock, fixture);
                const stockPng = await capture(join(output, `${stem}-stock-control.png`));
                const stockMismatch = differences(source, stockObservation);
                result.stockControl = { equal: !stockMismatch.length, screenshotEqual: sourcePng === stockPng, differences: stockMismatch.slice(0, 30), observations: stockObservation, readiness: stockReady, screenshot: stockPng };
            }
            await visit(routes.mutant, viewport, state, baseline);
            const mutated = await snapshot(baseline, fixture);
            const mutantPng = await capture(join(output, `${stem}-mutant.png`));
            result.mutant = { detected: differences(before, mutated).length > 0 && mutantPng !== beforePng, propertyDifferenceCount: differences(before, mutated).length, screenshot: mutantPng };
            if (!result.mutant.detected) throw Error(`Evaluation failed to detect deliberate CSS corruption: ${stem}`);
            await writeFile(join(output, `${stem}.json`), JSON.stringify(result, null, 2));
        }
        const viewport = fixture.viewports[0];
        for (let pair = 0; pair < (timingFixtures.includes(fixture.id) ? timingPairs : 0); pair++) {
            const order = pair % 2 ? ['compact', 'baseline'] : ['baseline', 'compact'];
            const sample = { pair, order, baseline: null, compact: null };
            for (const mode of order) sample[mode] = await timing(routes[mode], viewport, mode === 'baseline' ? baseline : candidate, `${fixture.id}-pair-${pair}-${mode}.trace.json.gz`);
            sample.deltaMs = Object.fromEntries(['recalcStyleMs', 'layoutMs', 'taskMs', 'paintMs'].map(key => [key, sample.compact[key] - sample.baseline[key]]));
            entry.timings.push(sample);
        }
        entry.timingSummary = Object.fromEntries(['recalcStyleMs', 'layoutMs', 'taskMs', 'paintMs'].map(key => [key, { baseline: summary(entry.timings.map(sample => sample.baseline[key])), compact: summary(entry.timings.map(sample => sample.compact[key])), pairedDelta: summary(entry.timings.map(sample => sample.deltaMs[key])) }]));
        entry.status = 'passed';
        console.log(`${fixture.id}: native computed styles + PNGs match; corrupt mutant detected; Brotli ${entry.size.baseline.total.brotli} → ${entry.size.compact.total.brotli} bytes`);
    }
    if (report.exceptions.length || report.unexpectedRequests.length) throw Error(`Unexpected page exceptions/remote requests: ${report.exceptions.length}/${report.unexpectedRequests.length}`);
    report.status = 'passed';
} catch (error) {
    report.status = 'failed';
    report.error = error.stack;
    process.exitCode = 1;
    console.error(error.message);
} finally {
    report.finishedAt = new Date().toISOString();
    await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2));
    if (page) page.close();
    if (browser) { await browser.command('Browser.close').catch(() => {}); browser.close(); }
    if (child && child.exitCode === null && child.signalCode === null) {
        await new Promise(resolveExit => {
            const finished = () => { clearTimeout(timer); resolveExit(); };
            const timer = setTimeout(() => { child.kill('SIGKILL'); resolveExit(); }, 3000);
            child.once('exit', finished);
            child.kill('SIGTERM');
        });
    }
    await new Promise(resolveClose => server.close(resolveClose));
    await rm(profile, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
}
console.log(`Raw report: ${join(output, 'report.json')}`);
