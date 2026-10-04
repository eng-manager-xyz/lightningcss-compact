// Paint events are native Chrome timeline intervals, not a task-duration proxy.
export function paintSummary(events, start, end) {
    if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) throw Error('Invalid native paint timing interval.');
    const mains = new Set(events.filter(event => event.ph === 'M' && event.name === 'thread_name' && event.args?.name === 'CrRendererMain').map(event => `${event.pid}:${event.tid}`));
    const slices = events.filter(event => event.name === 'Paint' && event.ph === 'X' && Number.isFinite(event.dur) && event.ts >= start && event.ts + event.dur <= end && mains.has(`${event.pid}:${event.tid}`));
    if (!slices.length) throw Error('No complete native renderer-main Paint slices in the fresh-document timing interval.');
    const threads = new Map();
    for (const event of slices) {
        const key = `${event.pid}:${event.tid}`;
        if (!threads.has(key)) threads.set(key, []);
        threads.get(key).push([event.ts, event.ts + event.dur]);
    }
    let unionMicroseconds = 0;
    for (const intervals of threads.values()) {
        intervals.sort(([left], [right]) => left - right);
        let [left, right] = intervals[0];
        for (const [nextLeft, nextRight] of intervals.slice(1)) {
            if (nextLeft <= right) right = Math.max(right, nextRight);
            else { unionMicroseconds += right - left; [left, right] = [nextLeft, nextRight]; }
        }
        unionMicroseconds += right - left;
    }
    return {
        source: 'devtools.timeline Paint/X on CrRendererMain',
        interval: { startMicroseconds: start, endMicroseconds: end },
        count: slices.length,
        unionMs: unionMicroseconds / 1000,
        inclusiveMs: slices.reduce((sum, event) => sum + event.dur, 0) / 1000,
        maxSliceMs: Math.max(...slices.map(event => event.dur)) / 1000,
        slices: slices.map(({ pid, tid, ts, dur }) => ({ pid, tid, ts, dur })),
    };
}
