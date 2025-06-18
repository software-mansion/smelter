import type {
  ApiRequest,
  MultipartRequest,
  SmelterManager,
  SetupInstanceOptions,
} from '@swmansion/smelter-core';

import { sendRequest, sendMultipartRequest } from './fetch';
import { retry, sleep } from './utils';
import { WebSocketConnection } from './ws';

export type InstanceOptions = {
  url: string | URL;
};

interface StatusResponse {
  queue_options?: { ahead_of_time_processing: boolean };
}

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
      let status = (await this.sendRequest({
        method: 'GET',
        route: '/status',
      })) as StatusResponse;

      const expectedAheadOfTime = opts.aheadOfTimeProcessing;

      if (status.queue_options?.ahead_of_time_processing !== expectedAheadOfTime) {
        opts.logger.warn(
          {
            expected: expectedAheadOfTime,
            actual: status.queue_options?.ahead_of_time_processing,
          },
          'Mismatch in queue_options.ahead_of_time_processing'
        );
      }
      return status;
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

export default RemoteInstanceManager;
