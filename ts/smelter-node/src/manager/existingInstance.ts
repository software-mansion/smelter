import type {
  ApiRequest,
  MultipartRequest,
  SmelterManager,
  SetupInstanceOptions,
} from '@swmansion/smelter-core';

import { sendRequest, sendMultipartRequest } from '../fetch';
import { retry, sleep } from '../utils';
import { WebSocketConnection } from '../ws';
import { getSmelterStatus } from '../getSmelterStatus';

type CreateInstanceOptions = {
  url: string | URL;
};

/**
 * SmelterManager that will connect to existing instance
 */
class ExistingInstanceManager implements SmelterManager {
  private url: URL;
  private wsConnection: WebSocketConnection;

  constructor(opts: CreateInstanceOptions) {
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

    const wsUrl = new URL('ws', url);
    wsUrl.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    this.wsConnection = new WebSocketConnection(wsUrl);
  }

  public async setupInstance(opts: SetupInstanceOptions): Promise<void> {
    await retry(async () => {
      await sleep(500);
      let smelterStatus = await getSmelterStatus(this);

      const expectedConfig = {
        aheadOfTimeProcessing: opts.aheadOfTimeProcessing,
      };

      const actualConfig = {
        aheadOfTimeProcessing: smelterStatus.configuration.aheadOfTimeProcessing,
      };

      for (const [key, expected] of Object.entries(expectedConfig)) {
        const actual = actualConfig[key as keyof typeof actualConfig];
        if (actual !== expected) {
          opts.logger.warn(
            {
              key,
              expected: expected === undefined ? 'undefined' : expected,
              actual: actual === undefined ? 'undefined' : actual,
            },
            `Mismatch between instance config and SDK.`
          );
        }
      }
      return smelterStatus;
    }, 10);
    await this.wsConnection.connect(opts.logger);
  }

  public async sendRequest(request: ApiRequest): Promise<object> {
    return await sendRequest(this.url, request);
  }

  async sendMultipartRequest(request: MultipartRequest): Promise<object> {
    return await sendMultipartRequest(this.url, request);
  }

  public registerEventListener(cb: (event: object) => void): void {
    this.wsConnection.registerEventListener(cb);
  }

  public async terminate(): Promise<void> {
    await this.wsConnection.close();
  }
}

export default ExistingInstanceManager;
