import path from 'path';
import type {
  ApiRequest,
  MultipartRequest,
  SmelterManager,
  SetupInstanceOptions,
} from '@swmansion/smelter-core';
import { WebSocketConnection } from './ws';

export type WorkerdSmelterInstanceOptions = {
  url: string | URL;
};

class WorkerdSmelterInstanceManager implements SmelterManager {
  private url: URL;
  private wsConnection: WebSocketConnection;

  constructor(opts: WorkerdSmelterInstanceOptions) {
    let url: URL;
    if (opts.url instanceof URL) {
      url = opts.url;
    } else {
      url = new URL(opts.url);
    }

    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      throw new Error('Expected url to use either http or https protocol');
    }

    this.url = url;

    const wsUrl = joinUrl(url, 'ws');
    wsUrl.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    this.wsConnection = new WebSocketConnection(wsUrl);
  }

  public async setupInstance(opts: SetupInstanceOptions): Promise<void> {
    await retry(async () => {
      await sleep(500);
      await this.sendRequest({
        method: 'GET',
        route: '/status',
      });
    }, 10);

    await this.sendRequest({
      method: 'POST',
      route: '/api/reset',
      body: {},
    });
    opts.logger.info('Sent reset request to the Smelter instance.');

    await this.wsConnection.connect(opts.logger);
  }

  public async sendRequest(request: ApiRequest): Promise<object> {
    const response = await fetch(joinUrl(this.url, request.route), {
      method: request.method,
      body: request.body && JSON.stringify(request.body),
      headers: {
        ...request.headers,
        'Content-Type': 'application/json',
      },
      keepalive: true,
    });
    if (response.status >= 400) {
      const err: any = new Error(`Request to Smelter server failed.`);
      err.response = response;
      err.body = await readErrorBody(response);
      throw err;
    }
    return (await response.json()) as object;
  }

  async sendMultipartRequest(request: MultipartRequest): Promise<object> {
    const response = await fetch(joinUrl(this.url, request.route), {
      method: request.method,
      body: request.body,
      keepalive: true,
      headers: request.headers,
    });

    if (response.status >= 400) {
      const err: any = new Error(`Request to Smelter server failed.`);
      err.response = response;
      err.body = await readErrorBody(response);
      throw err;
    }
    return (await response.json()) as object;
  }

  public registerEventListener(cb: (event: object) => void): void {
    this.wsConnection.registerEventListener(cb);
  }

  public async terminate(): Promise<void> {
    await this.wsConnection.close();
  }
}

function joinUrl(base: URL | string, relative: string): URL {
  const url = new URL(base);
  url.pathname = path.join(url.pathname, relative);
  return url;
}

async function readErrorBody(response: Response): Promise<object | string> {
  const body = await response.text();
  try {
    return JSON.parse(body);
  } catch {
    return body;
  }
}

async function sleep(timeoutMs: number): Promise<void> {
  await new Promise<void>(res => {
    setTimeout(() => {
      res();
    }, timeoutMs);
  });
}

async function retry<T>(fn: () => Promise<T>, retry: number): Promise<T> {
  let count = 0;
  while (true) {
    count += 1;
    try {
      return await fn();
    } catch (err) {
      if (count > retry) {
        throw err;
      }
    }
  }
}

export default WorkerdSmelterInstanceManager;
