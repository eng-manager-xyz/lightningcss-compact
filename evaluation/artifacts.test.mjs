import test from 'node:test';
import assert from 'node:assert/strict';
import { artifactReport, artifactSize, differences, summary } from './artifacts.mjs';
import { fixtures } from './fixtures.mjs';

test('compression uses real independent artifact boundaries and deterministic bytes', () => {
    const css = '.a{color:red;height:100px}';
    const html = '<div class="a">left</div><div class="a">right</div>';
    const report = artifactReport({ 'fixture.css': css }, { document: html });
    assert.equal(report.total.brotli, artifactSize(css).brotli + artifactSize(html).brotli);
    assert.deepEqual(report, artifactReport({ 'fixture.css': css }, { document: html }));
    assert.equal(report.artifacts['html:document'].sha256.length, 64);
});
test('non-HTML bindings use the same explicit synthetic literal stream as the compiler', () => {
    const report = artifactReport({}, { z: 'b', a: 'a' }, { z: 'token', a: 'selector' });
    assert.deepEqual(report.artifacts['javascript:binding-literals'], artifactSize('"a";"b";'));
});
test('deep comparison detects wrong property, missing node, and geometry', () => {
    const before = { probe: { style: { color: 'rgb(255, 0, 0)' }, rect: [0, 0, 50, 100] } };
    assert.deepEqual(differences(before, structuredClone(before)), []);
    assert.ok(differences(before, { probe: { style: { color: 'blue' }, rect: [0, 0, 50, 101] } }).length === 2);
    assert.ok(differences(before, {}).length);
});
test('timing medians average the central pair for even sample counts', () => {
    assert.equal(summary([4, 1, 3, 2]).median, 2.5);
    assert.equal(summary([1, 5, 3]).median, 3);
    assert.equal(summary([]), null);
});
test('evaluation fixtures include known dangerous cascade and dynamic cases', () => {
    const all = fixtures();
    assert.equal(new Set(all.map(value => value.id)).size, all.length);
    for (const id of ['shared-bundle', 'noncontiguous-bundle', 'ordered-override', 'shorthand-logical', 'layer-priority', 'pseudo-and-has', 'dynamic-token', 'reserved-attribute', 'incomplete-inventory']) assert.ok(all.some(value => value.id === id));
    assert.ok(all.every(value => value.input.stylesheets.length && value.input.bindings.some(binding => binding.id === 'document') && value.input.load_groups.length));
});
