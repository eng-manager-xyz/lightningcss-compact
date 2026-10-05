import test from 'node:test';
import assert from 'node:assert/strict';
import { paintSummary } from './paint.mjs';

test('paint uses native renderer-main slices and unions nested intervals', () => {
    const events = [
        { ph: 'M', name: 'thread_name', pid: 1, tid: 2, args: { name: 'CrRendererMain' } },
        { ph: 'X', name: 'Paint', pid: 1, tid: 2, ts: 120, dur: 20 },
        { ph: 'X', name: 'Paint', pid: 1, tid: 2, ts: 125, dur: 5 },
        { ph: 'X', name: 'Paint', pid: 1, tid: 2, ts: 150, dur: 10 },
        { ph: 'X', name: 'Paint', pid: 1, tid: 3, ts: 120, dur: 100 },
        { ph: 'X', name: 'Task', pid: 1, tid: 2, ts: 100, dur: 200 },
        { ph: 'X', name: 'Paint', pid: 1, tid: 2, ts: 90, dur: 10 },
    ];
    const result = paintSummary(events, 100, 200);
    assert.equal(result.count, 3);
    assert.equal(result.unionMs, .03);
    assert.equal(result.inclusiveMs, .035);
});
test('a task duration never substitutes for missing paint evidence', () => {
    assert.throws(() => paintSummary([{ ph: 'X', name: 'Task', ts: 120, dur: 20 }], 100, 200), /No complete native/);
    assert.throws(() => paintSummary([], 200, 100), /Invalid/);
});
