import test from 'node:test';
import assert from 'node:assert/strict';
import { exhaustiveOracle, oracleProblem } from './oracle.mjs';

test('independent exhaustive oracle joins always-cooccurring properties and elides unobserved anchors', () => {
    const result = exhaustiveOracle(oracleProblem(2));
    assert.equal(result.domain.candidates, 12);
    assert.ok(result.leafCount > 100);
    assert.deepEqual(result.best.cover, ['atom:left+right:red+height']);
    assert.equal(result.best.classes.left, result.best.classes.right);
    assert.match(result.best.stylesheets['fixture.css'], /^\.[a-d]\{(?:color:red;height:100px|height:100px;color:red)\}$/);
    assert.ok(result.best.measurement.total.brotli < result.original.total.brotli);
    assert.equal(result.best.classes.left.split(' ').length, 1);
});
test('single-declaration optimum uses one shared token', () => {
    const result = exhaustiveOracle(oracleProblem(1));
    assert.deepEqual(result.best.cover, ['atom:left+right:red']);
    assert.equal(result.best.classes.left, result.best.classes.right);
    assert.ok(result.best.measurement.total.raw < result.original.total.raw);
});
test('oracle refuses larger kernels rather than mislabeling a bounded search optimal', () => {
    assert.throws(() => exhaustiveOracle({ ...oracleProblem(), alphabet: ['a', 'b', 'c', 'd', 'e'] }), /limits exceeded/);
    assert.throws(() => exhaustiveOracle({ ...oracleProblem(), units: [{ id: 'a', css: 'width:1px' }, { id: 'b', css: 'height:1px' }, { id: 'c', css: 'color:red' }] }), /twelve finite candidates/);
});
