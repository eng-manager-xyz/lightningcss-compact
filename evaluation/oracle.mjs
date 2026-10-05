// Independent exhaustive oracle for a deliberately small, declared domain.
// It does not import optimizer code or use heuristic compressed-byte bounds.
import { artifactReport } from './artifacts.mjs';

function subsets(values) {
    return Array.from({ length: 2 ** values.length - 1 }, (_, index) => values.filter((_, bit) => (index + 1) & (1 << bit)));
}
function* permutations(values) {
    if (values.length < 2) { yield values; return; }
    for (let index = 0; index < values.length; index++) {
        for (const rest of permutations([...values.slice(0, index), ...values.slice(index + 1)])) yield [values[index], ...rest];
    }
}
function* product(choices, index = 0, current = []) {
    if (index === choices.length) { yield current; return; }
    for (const choice of choices[index]) yield* product(choices, index + 1, [...current, choice]);
}
export function oracleProblem(propertyCount = 2) {
    if (![1, 2].includes(propertyCount)) throw Error('Only the two declared independent oracle fixtures are defined.');
    return {
        owners: ['left', 'right'],
        units: [{ id: 'red', css: 'color:red' }, ...propertyCount === 2 ? [{ id: 'height', css: 'height:100px' }] : []],
        html: '<main><div data-probe="left" class="{{left}}">left</div><div data-probe="right" class="{{right}}">right</div></main>',
        alphabet: ['a', 'b', 'c', 'd'],
    };
}
export function exhaustiveOracle(problem) {
    const { owners, units, alphabet, html } = problem;
    if (owners.length * units.length > 6 || alphabet.length > 4) throw Error('Oracle limits exceeded: six declaration occurrences, four symbols.');
    const cells = owners.flatMap(owner => units.map(unit => `${owner}:${unit.id}`));
    const candidates = [];
    for (const members of subsets(owners)) for (const declarations of subsets(units)) {
        const coverage = members.flatMap(owner => declarations.map(unit => `${owner}:${unit.id}`));
        candidates.push({ id: `group:${members.join('+')}:${declarations.map(unit => unit.id).join('+')}`, kind: 'group', members, declarations, coverage });
        // Single-owner styles use their residual anchor, never an extra atom.
        if (members.length > 1) candidates.push({ id: `atom:${members.join('+')}:${declarations.map(unit => unit.id).join('+')}`, kind: 'atom', members, declarations, coverage });
    }
    if (candidates.length > 12) throw Error('Oracle limit exceeded: twelve finite candidates.');
    let leafCount = 0, feasibleCovers = 0, best = null;
    const original = {
        stylesheets: { 'fixture.css': owners.map(owner => `.${owner}{${units.map(unit => unit.css).join(';')}}`).join('') },
        bindings: { document: html.replace(/\{\{(\w+)\}\}/g, (_, owner) => owner) },
    };
    function* covers(remaining, chosen = []) {
        if (!remaining.size) { yield chosen; return; }
        const first = [...remaining][0];
        for (const candidate of candidates) {
            if (!candidate.coverage.includes(first) || candidate.coverage.some(cell => !remaining.has(cell))) continue;
            yield* covers(new Set([...remaining].filter(cell => !candidate.coverage.includes(cell))), [...chosen, candidate]);
        }
    }
    for (const cover of covers(new Set(cells))) {
        const anchors = owners.filter(owner => cover.some(candidate => candidate.kind === 'group' && candidate.members.includes(owner)));
        const symbols = [...anchors.map(owner => `owner:${owner}`), ...cover.filter(candidate => candidate.kind === 'atom').map(candidate => candidate.id)];
        if (symbols.length > alphabet.length) continue;
        feasibleCovers++;
        // Finite names are exactly the first K declared alphabet entries.
        for (const names of permutations(alphabet.slice(0, symbols.length))) {
            const naming = Object.fromEntries(symbols.map((symbol, index) => [symbol, names[index]]));
            const classes = Object.fromEntries(owners.map(owner => [owner, [
                ...anchors.includes(owner) ? [naming[`owner:${owner}`]] : [],
                ...cover.filter(candidate => candidate.kind === 'atom' && candidate.members.includes(owner)).map(candidate => naming[candidate.id]).sort(),
            ].join(' ')]));
            const document = html.replace(/\{\{(\w+)\}\}/g, (_, owner) => classes[owner]);
            for (const ruleOrder of permutations(cover)) {
                for (const declarationOrders of product(ruleOrder.map(candidate => [...permutations(candidate.declarations)]))) {
                    const css = ruleOrder.map((candidate, index) => {
                        const selector = candidate.kind === 'atom' ? `.${naming[candidate.id]}` : candidate.members.map(owner => `.${naming[`owner:${owner}`]}`).join(',');
                        return `${selector}{${declarationOrders[index].map(unit => unit.css).join(';')}}`;
                    }).join('');
                    const stylesheets = { 'fixture.css': css }, bindings = { document };
                    const measured = artifactReport(stylesheets, bindings);
                    const score = [measured.total.brotli, measured.total.gzip, measured.total.raw, css, document];
                    leafCount++;
                    if (!best || compareScore(score, best.score) < 0) best = { score, stylesheets, bindings, classes, naming, cover: cover.map(candidate => candidate.id), measurement: measured };
                }
            }
        }
    }
    return {
        claim: 'Exact minimum only within the finite candidate rectangles, first-K alphabet permutations, rule/declaration permutations, and anchor-first/sorted-atom HTML token ordering enumerated here.',
        domain: { declarationOccurrences: cells.length, candidates: candidates.length, maxSymbols: alphabet.length, alphabet, pruning: 'Only invalid coverage or too many symbols; no heuristic compression bound.' },
        leafCount, feasibleCovers, original: artifactReport(original.stylesheets, original.bindings), best,
    };
}
function compareScore(left, right) {
    for (let index = 0; index < left.length; index++) {
        if (left[index] === right[index]) continue;
        return left[index] < right[index] ? -1 : 1;
    }
    return 0;
}
