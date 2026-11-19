interface IpcReply {
  id: string;
  ok: boolean;
  data: unknown;
  error: string | null;
}

type EventListener = (data: unknown) => void;

interface PendingCall {
  resolve: (value: unknown) => void;
  reject: (err: Error) => void;
}

class AppBridge {
  private seq = 0;
  private pending = new Map<string, PendingCall>();
  private listeners = new Map<string, Set<EventListener>>();

  constructor() {
    (window as unknown as { __APP_BRIDGE__: unknown }).__APP_BRIDGE__ = {
      onResponse: (reply: IpcReply) => {
        const entry = this.pending.get(reply.id);
        if (!entry) return;
        this.pending.delete(reply.id);
        if (reply.ok) {
          entry.resolve(reply.data);
        } else {
          entry.reject(new Error(reply.error || 'Lỗi không xác định từ lõi xử lý'));
        }
      },
      onEvent: (eventName: string, data: unknown) => {
        const set = this.listeners.get(eventName);
        if (!set) return;
        for (const fn of set) {
          try {
            fn(data);
          } catch {
            // ignore listener error
          }
        }
      },
    };
  }

  public isNativeHost(): boolean {
    return typeof (window as unknown as { ipc?: { postMessage?: unknown } }).ipc?.postMessage === 'function';
  }

  public on(eventName: string, cb: EventListener): () => void {
    let set = this.listeners.get(eventName);
    if (!set) {
      set = new Set();
      this.listeners.set(eventName, set);
    }
    set.add(cb);
    return () => {
      set?.delete(cb);
    };
  }

  public invoke<T>(cmd: string, payload: unknown = {}): Promise<T> {
    const nativeIpc = (window as unknown as { ipc?: { postMessage: (msg: string) => void } }).ipc;
    if (!nativeIpc?.postMessage) {
      return Promise.reject(new Error('Chưa kết nối với lõi Rust WebView2 IPC'));
    }

    const id = `req_${++this.seq}_${Date.now()}`;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: resolve as (v: unknown) => void,
        reject,
      });
      nativeIpc.postMessage(JSON.stringify({ id, cmd, payload }));
    });
  }
}

export const bridge = new AppBridge();
