import type {
  ApiRequest,
  MultipartRequest,
  SmelterManager,
  SetupInstanceOptions,
} from '@swmansion/smelter-core';

import { sendRequest, sendMultipartRequest } from './fetch';
import { retry, sleep } from './utils';
import { WebSocketConnection } from './ws';
import { getSmelterStatus } from './getSmelterStatus';

export type InstanceOptions = {
  url: string | URL;
};

class RemoteInstanceManager implements SmelterManager {
  private url: URL;
  private wsConnection: WebSocketConnection;

  constructor(opts: InstanceOptions) {
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

      const expectedAheadOfTimeProcessing = opts.aheadOfTimeProcessing;
      const actualAheadOfTimeProcessing = smelterStatus.configuration.aheadOfTimeProcessing;

      if (actualAheadOfTimeProcessing !== expectedAheadOfTimeProcessing) {
        opts.logger.warn(
          {
            expected: expectedAheadOfTimeProcessing,
            actual: actualAheadOfTimeProcessing,
          },
          'Mismatch in aheadOfTimeProcessing'
        );
      }
      return smelterStatus;
    }, 10);

    try {
      await this.sendRequest({
        method: 'POST',
        route: '/api/reset',
        body: {},
      });
      opts.logger.info('Sent reset request to existing Smelter instance.');
    } catch (err) {
      opts.logger.warn({ err }, 'Failed to reset existing Smelter instance.');
    }

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

export default RemoteInstanceManager;
