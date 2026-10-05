// Repository-owned local Chrome transport, adapted from
// https://github.com/matthewharwood/engmanager.xyz/blob/main/scripts/local-chrome-cdp.mjs
// Original author: Matthew Harwood and contributors. This authorized adaptation
// is distributed under this package's MIT OR Apache-2.0 license.
export class Cdp {
    constructor(url) {
        this.socket = new WebSocket(url);
        this.nextId = 0;
        this.pending = new Map();
        this.listeners = new Map();
        this.open = new Promise((resolve, reject) => {
            this.socket.addEventListener('open', resolve, { once: true });
            this.socket.addEventListener('error', reject, { once: true });
        });
        this.socket.addEventListener('message', ({ data }) => {
            const message = JSON.parse(data);
            if (message.id) {
                const pending = this.pending.get(message.id);
                if (!pending) return;
                this.pending.delete(message.id);
                clearTimeout(pending.timer);
                if (message.error) pending.reject(Object.assign(new Error(`${pending.method}: ${message.error.message}`), { cdp: message.error }));
                else pending.resolve(message.result);
            } else this.listeners.get(message.method)?.forEach(listener => listener(message.params));
        });
        this.socket.addEventListener('close', () => {
            for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(new Error(`Chrome disconnected during ${pending.method}`)); }
            this.pending.clear();
        });
    }
    async command(method, params = {}, timeoutMs = 15000) {
        await this.open;
        if (this.socket.readyState !== WebSocket.OPEN) throw Error(`Chrome connection closed before ${method}`);
        const id = ++this.nextId;
        const result = new Promise((resolve, reject) => {
            const timer = setTimeout(() => { this.pending.delete(id); reject(Error(`Chrome command deadline: ${method}`)); }, timeoutMs);
            this.pending.set(id, { resolve, reject, method, timer });
        });
        this.socket.send(JSON.stringify({ id, method, params }));
        return result;
    }
    on(method, listener) {
        if (!this.listeners.has(method)) this.listeners.set(method, new Set());
        this.listeners.get(method).add(listener);
        return () => this.listeners.get(method).delete(listener);
    }
    close() { this.socket.close(); }
}
