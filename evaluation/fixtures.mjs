const box = (probe, classes, text = probe) => `<div data-probe="${probe}" class="${classes}">${text}</div>`;
const canvas = markup => `<main>${markup}</main>`;
function fixture(id, css, markup, managedClasses, options = {}) {
    return {
        id,
        input: {
            stylesheets: [{ id: 'fixture.css', source: css }],
            bindings: [{ id: 'document', kind: 'html', value: canvas(markup) }, ...(options.bindings || [])],
            complete_usage: options.completeUsage ?? true,
            managed_classes: managedClasses,
            dynamic_classes: options.dynamicClasses || [],
            reserved_classes: options.reservedClasses || [],
            reserved_prefixes: options.reservedPrefixes || [],
            load_groups: [['fixture.css']],
        },
        states: options.states || [{ id: 'default', actions: [] }],
        viewports: options.viewports || [{ id: 'desktop', width: 960, height: 720 }, { id: 'mobile', width: 390, height: 844 }],
        requiredSupport: options.requiredSupport || [],
        expectedStyles: options.expectedStyles || {},
        ...options.expected && { expected: options.expected },
    };
}

export function fixtures() {
    const cssPair = '.left{color:red;height:100px}.right{color:red;height:100px}';
    return [
        fixture('shared-single', '.left{color:red}.right{color:red}', box('left', 'left') + box('right', 'right'), ['left', 'right']),
        fixture('shared-bundle', cssPair, box('left', 'left') + box('right', 'right'), ['left', 'right'], { expected: { sharedCompleteBinding: true } }),
        fixture('noncontiguous-bundle', '.left{color:red;width:130px;height:100px}.right{color:red;width:170px;height:100px}', box('left', 'left') + box('right', 'right'), ['left', 'right']),
        fixture('partial-support', '.left{color:red;height:100px}.right{color:red;height:100px}.third{color:red;width:150px}', box('left', 'left') + box('right', 'right') + box('third', 'third'), ['left', 'right', 'third']),
        fixture('ordered-override', '.first{color:red}.middle{color:blue}.last{color:red}', box('all', 'first middle last') + box('without-last', 'first middle'), ['first', 'middle', 'last']),
        fixture('upstream-stale-style-index', '.left{color:red;height:100px}.right{color:red}.right{height:100px}.tail{color:blue}', box('left', 'left') + box('right', 'right') + box('tail', 'tail'), ['left', 'right', 'tail'], {
            dynamicClasses: ['left', 'right', 'tail'],
            reservedClasses: ['left', 'right', 'tail'],
            expectedStyles: { left: { color: 'rgb(255, 0, 0)', height: '100px' }, right: { color: 'rgb(255, 0, 0)', height: '100px' }, tail: { color: 'rgb(0, 0, 255)' } },
        }),
        fixture('shorthand-logical', '.left{border:5px solid red;border-left-color:blue;padding:4px;padding-inline-start:12px;width:140px}.right{border:5px solid red;border-left-color:blue;padding:4px;padding-inline-start:12px;width:140px}', box('left', 'left') + box('right', 'right'), ['left', 'right'], {
            states: [{ id: 'ltr', actions: [] }, { id: 'rtl', actions: [{ type: 'direction', value: 'rtl' }] }],
        }),
        ...['scroll-margin', 'scroll-padding'].map(property => fixture(`${property}-logical-conflict`, `.first{${property}-inline-start:10px}.middle{${property}-left:20px}.last{${property}-inline-start:10px}`, box('middle-last', 'middle last') + box('first-middle', 'first middle') + box('all', 'first middle last'), ['first', 'middle', 'last'], {
            requiredSupport: [`${property}-inline-start:10px`, `${property}-left:20px`],
            expectedStyles: { 'middle-last': { [`${property}-inline-start`]: '10px' }, all: { [`${property}-inline-start`]: '10px' } },
            states: [
                { id: 'ltr', actions: [] },
                { id: 'rtl', actions: [{ type: 'direction', value: 'rtl' }] },
                { id: 'vertical-rl', actions: [{ type: 'writing-mode', value: 'vertical-rl' }] },
                { id: 'vertical-lr-rtl', actions: [{ type: 'writing-mode', value: 'vertical-lr' }, { type: 'direction', value: 'rtl' }] },
            ],
        })),
        ...[
            ['animation-range-longhand-conflict', 'animation-range-start:10%', 'animation-range:20% 80%'],
            // Chromium exposes this axis longhand through its supported
            // vendor alias. The standard shorthand resets that same axis.
            ['mask-position-longhand-conflict', '-webkit-mask-position-x:10px', 'mask-position:20px 30px'],
        ].map(([id, longhand, shorthand]) => fixture(id, `.first{${longhand}}.middle{${shorthand}}.last{${longhand}}`, box('middle-last', 'middle last') + box('first-middle', 'first middle') + box('all', 'first middle last'), ['first', 'middle', 'last'], {
            requiredSupport: [longhand, shorthand],
            expectedStyles: { 'middle-last': { [longhand.split(':')[0]]: longhand.split(':')[1] }, all: { [longhand.split(':')[0]]: longhand.split(':')[1] } },
        })),
        fixture('fallback-custom', '.left{--tone:red;display:block;display:grid;display:unsupported-value;color:red;color:var(--tone);height:100px}.right{--tone:red;display:block;display:grid;display:unsupported-value;color:red;color:var(--tone);height:100px}', box('left', 'left') + box('right', 'right'), ['left', 'right']),
        fixture('layer-priority', '@layer early,late;@layer early{.left{color:red!important;height:60px}}@layer late{.left{color:blue!important;height:100px}.right{color:blue!important;height:100px}}.left,.right{height:80px;color:green}', box('left', 'left') + box('right', 'right'), ['left', 'right']),
        fixture('conditions', '.left,.right{height:100px;color:red}@media(max-width:500px){.left{height:120px}.right{height:140px}}@supports(display:grid){.left,.right{display:grid}}@media(prefers-reduced-motion:reduce){.left,.right{color:blue}}', box('left', 'left') + box('right', 'right'), ['left', 'right'], {
            states: [{ id: 'normal', actions: [] }, { id: 'reduced', actions: [{ type: 'media', name: 'prefers-reduced-motion', value: 'reduce' }] }],
        }),
        fixture('pseudo-and-has', '.left,.right{color:red;height:100px}.left:hover{color:blue}.right:focus-visible{outline:3px solid green}.left::before,.right::before{content:"mark";display:block;height:16px}.frame:has(>.left){border:4px solid purple}.frame :where(.left,.right){width:180px}', `<section class="frame" data-probe="frame">${box('left', 'left') + '<button data-probe="right" class="right">Right</button>'}</section>`, ['left', 'right', 'frame'], {
            states: [{ id: 'default', actions: [] }, { id: 'hover', actions: [{ type: 'hover', probe: 'left' }] }, { id: 'focus', actions: [{ type: 'focus', probe: 'right' }] }],
        }),
        fixture('dynamic-token', '.left,.right{color:red;height:100px}.is-hidden{display:none}.left.is-active{color:blue}', box('left', 'left') + box('right', 'right'), ['left', 'right', 'is-hidden', 'is-active'], {
            dynamicClasses: ['is-hidden', 'is-active'],
            bindings: [{ id: 'hidden', kind: 'token', value: 'is-hidden' }, { id: 'active', kind: 'token', value: 'is-active' }, { id: 'selector', kind: 'selector', value: '.left:has(+ .right)' }],
            states: [{ id: 'default', actions: [] }, { id: 'active', actions: [{ type: 'toggle', probe: 'left', logical: 'is-active', binding: 'active' }] }, { id: 'hidden', actions: [{ type: 'toggle', probe: 'right', logical: 'is-hidden', binding: 'hidden' }] }],
        }),
        fixture('reserved-attribute', '.left,.right{color:red;height:100px}.a{color:green;width:113px}pre[class*="language-"]{color:purple;border:2px solid orange}', box('left', 'left') + box('right', 'right') + box('foreign', 'a') + '<pre data-probe="code" class="language-rust">let x = 1;</pre>', ['left', 'right'], { reservedClasses: ['a'], reservedPrefixes: ['language-'] }),
        fixture('foreign-html-only', '.left{color:red;height:100px}.right{color:red;height:100px}', box('left', 'left') + box('right', 'right') + box('foreign', 'a'), ['left', 'right']),
        fixture('owned-attribute-prefix', '.owned-left{height:100px}.owned-right{height:100px}div[class^="owned-"]{color:green}', box('left', 'owned-left') + box('right', 'owned-right'), ['owned-left', 'owned-right']),
        fixture('attribute-introduced-match', '.left{color:red;height:100px}.right{color:red;height:100px}[class^="a"]{border:5px solid blue}', box('left', 'left') + box('right', 'right'), ['left', 'right']),
        fixture('attribute-whitespace', '.left{height:100px}.right{height:100px}[class="left  right"]{color:green}.left.right:not([class="left  right"]){color:red}', box('exact', 'left  right') + box('normalized', 'left right') + box('duplicates', 'left left  right'), ['left', 'right']),
        fixture('attribute-boundary-whitespace', '.barReallyLongManagedClass{height:100px}[class$="foo"]{color:red}[class^="foo"]{border:5px solid purple}', Array.from({ length: 10 }, (_, index) => box(index ? `trailing-${index}` : 'trailing', 'barReallyLongManagedClass foo ')).join('') + Array.from({ length: 10 }, (_, index) => box(index ? `leading-${index}` : 'leading', ' foo barReallyLongManagedClass')).join('') + box('both', ' foo ') + box('actual-suffix', 'other foo'), ['barReallyLongManagedClass'], {
            bindings: [{ id: 'boundaries', kind: 'selector', value: '[class$="foo"],[class^="foo"]' }],
            expectedStyles: { trailing: { color: 'rgb(0, 0, 0)', 'border-left-width': '0px' }, leading: { color: 'rgb(0, 0, 0)', 'border-left-width': '0px' }, both: { color: 'rgb(0, 0, 0)', 'border-left-width': '0px' } },
        }),
        fixture('attribute-selector-hook', '.left{color:red;height:100px}.right{color:red;height:100px}', box('left', 'left') + box('right', 'right'), ['left', 'right'], { bindings: [{ id: 'hook', kind: 'selector', value: '[class~="left"]' }, { id: 'nested-hook', kind: 'selector', value: ':is([class~="right"],.left):not(:has(>.missing))' }] }),
        fixture('empty-attribute-values', '.left{color:red;height:100px}.right{color:red;height:100px}[class^=""]{color:blue}[class*=""]{display:none}[class$=""]{height:20px}', box('left', 'left') + box('right', 'right'), ['left', 'right'], { bindings: [{ id: 'empty-hook', kind: 'selector', value: '[class^=""],[class*=""],[class$=""]' }] }),
        fixture('noscript-raw-content', cssPair, box('left', 'left') + box('right', 'right') + '<noscript data-probe="noscript"><div data-probe="fallback-left" class="left">Static fallback &amp; literal markup.</div><div data-probe="fallback-foreign" class="a">Foreign fallback.</div></noscript>', ['left', 'right'], {
            expectedStyles: { left: { color: 'rgb(255, 0, 0)', height: '100px' }, right: { color: 'rgb(255, 0, 0)', height: '100px' } },
            states: [
                { id: 'scripting-enabled', actions: [] },
                { id: 'scripting-disabled', scriptDisabled: true, actions: [], expectedStyles: { 'fallback-left': { color: 'rgb(255, 0, 0)', height: '100px' }, 'fallback-foreign': { color: 'rgb(0, 0, 0)' } } },
            ],
        }),
        fixture('scope-prelude-and-end', '@scope (.frame) to (.stop){.left{color:red;height:100px}.right{color:red;height:100px}}', `<section class="frame" data-probe="frame">${box('inside-left', 'left') + box('inside-right', 'right')}<section class="stop" data-probe="stop">${box('after-stop', 'left')}</section></section>${box('outside', 'right')}`, ['left', 'right', 'frame', 'stop'], {
            expectedStyles: { 'inside-left': { color: 'rgb(255, 0, 0)', height: '100px' }, 'inside-right': { color: 'rgb(255, 0, 0)', height: '100px' }, 'after-stop': { color: 'rgb(0, 0, 0)' }, outside: { color: 'rgb(0, 0, 0)' } },
        }),
        fixture('supports-selector-prelude', '.left{color:red;height:100px}.right{color:red;height:100px}@supports selector(.feature){.left,.right{border:3px solid green}}', box('left', 'left') + box('right', 'right') + box('feature', 'feature'), ['left', 'right', 'feature'], {
            requiredSupport: ['selector(.feature)'], expectedStyles: { left: { 'border-left-width': '3px', 'border-left-color': 'rgb(0, 128, 0)' }, right: { 'border-left-width': '3px', 'border-left-color': 'rgb(0, 128, 0)' } },
        }),
        fixture('escaped-identifiers', '.hover\\:red{color:red;height:100px}.next\\.right{color:red;height:100px}', box('left', 'hover:red') + box('right', 'next.right'), ['hover:red', 'next.right']),
        fixture('nesting', '.left{color:red;height:100px;&:hover{color:blue}}.right{color:red;height:100px;&::before{content:"nested"}}', box('left', 'left') + box('right', 'right'), ['left', 'right'], { states: [{ id: 'default', actions: [] }, { id: 'hover', actions: [{ type: 'hover', probe: 'left' }] }] }),
        fixture('incomplete-inventory', cssPair, box('left', 'left') + box('right', 'right'), ['left', 'right'], { completeUsage: false }),
        fixture('timing-many-nodes', cssPair, Array.from({ length: 512 }, (_, index) => box(`box-${index}`, index % 2 ? 'left' : 'right', 'Shared declarations')).join(''), ['left', 'right'], { viewports: [{ id: 'desktop', width: 960, height: 720 }] }),
    ];
}
