import { type Logger } from 'pino';

export class WebSocketConnection {
  private url: URL;
  private listeners: Set<(event: object) => void>;
  private ws: WebSocket | null = null;
  private donePromise?: Promise<void>;

  constructor(url: URL) {
    this.url = url;
    this.listeners = new Set();
  }

  public async connect(logger: Logger): Promise<void> {
    const ws = new WebSocket(this.url);

    let connected = false;
    await new Promise<void>((res, rej) => {
      ws.addEventListener('error', (err: any) => {
        if (connected) {
          logger.error(err, 'WebSocket error');
        } else {
          rej(err);
        }
      });

      ws.addEventListener('open', () => {
        connected = true;
        res();
      });

      ws.addEventListener('message', data => {
        const event = parseEvent(data, logger);
        if (event) {
          for (const listener of this.listeners) {
            listener(event);
          }
        }
      });

      this.donePromise = new Promise(res => {
        ws.addEventListener('close', () => {
          this.ws = null;
          res();
        });
      });
    });
    this.ws = ws;
  }

  public registerEventListener(cb: (event: object) => void): void {
    this.listeners.add(cb);
  }

  public async close(): Promise<void> {
    this.ws?.close();
    await this.donePromise;
  }
}

function parseEvent(event: MessageEvent, logger: Logger): unknown {
  try {
    return JSON.parse(event.data);
  } catch (err: any) {
    logger.warn(err, `Invalid event received ${event}`);
    return null;
  }
}
