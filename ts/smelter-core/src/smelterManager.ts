import type { Logger } from 'pino';
import type { ApiRequest, MultipartRequest } from './api';

export interface SetupInstanceOptions {
  /**
   * sets SMELTER_AHEAD_OF_TIME_PROCESSING_ENABLE environment variable.
   */
  aheadOfTimeProcessing: boolean;

  logger: Logger;
}

export interface SmelterManager {
  setupInstance(opts: SetupInstanceOptions): Promise<void>;
  sendRequest(request: ApiRequest): Promise<object>;
  sendMultipartRequest(request: MultipartRequest): Promise<object>;
  registerEventListener(cb: (event: unknown) => void): void;
  terminate(): Promise<void>;
}
