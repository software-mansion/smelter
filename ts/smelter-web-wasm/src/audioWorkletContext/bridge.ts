import type { Logger } from 'pino';
import type { AudioWorkletMessage } from './workletApi';

type RequestMessage = {
  id: string;
  request: AudioWorkletMessage;
};

type ResponseMessage = {
  type: 'workerResponse';
  id: string;
  response?: boolean;
  error?: Error;
};

type PendingMessage = {
  res: (response: boolean) => void;
  rej: (err: Error) => void;
};

let requestCounter = 1;

export function listenForMessages(
  port: MessagePort,
  onMessage: (request: AudioWorkletMessage) => Promise<boolean>
) {
  port.onmessage = async (event: MessageEvent<RequestMessage>) => {
    try {
      const response = await onMessage(event.data.request);
      port.postMessage({
        type: 'workerResponse',
        id: event.data.id,
        response,
      } as ResponseMessage);
    } catch (error: any) {
      port.postMessage({
        type: 'workerResponse',
        id: event.data.id,
        error,
      } as ResponseMessage);
    }
  };
}

export class AudioWorkletMessagePort {
  private port: MessagePort;
  private pendingMessages: Record<string, PendingMessage> = {};
  private logger: Logger;
  private closed: boolean = false;

  constructor(port: MessagePort, logger: Logger) {
    this.logger = logger;
    this.port = port;
    this.port.onmessage = (event: MessageEvent<ResponseMessage>) => {
      if (event.data.type === 'workerResponse') {
        this.handleResponse(event.data);
      }
    };
  }

  public async postMessage(
    request: AudioWorkletMessage,
    transferable?: Transferable[]
  ): Promise<boolean> {
    if (this.closed) {
      return false;
    }
    const requestId = String(requestCounter);
    requestCounter += 1;

    const pendingMessage: PendingMessage = {} as any;
    const responsePromise = new Promise<boolean>((res, rej) => {
      pendingMessage.res = res;
      pendingMessage.rej = rej;
    });
    this.pendingMessages[requestId] = pendingMessage;

    if (transferable) {
      this.port.postMessage({ id: requestId, request }, transferable);
    } else {
      this.port.postMessage({ id: requestId, request });
    }
    return await responsePromise;
  }

  public terminate() {
    this.port.close();
    this.closed = true;
    for (const pending of Object.values(this.pendingMessages)) {
      pending.res(false);
    }
    this.pendingMessages = {};
  }

  private handleResponse(msg: ResponseMessage) {
    const pendingMessage = this.pendingMessages[msg.id];
    if (!pendingMessage) {
      this.logger.error(`Unknown response from Web Worker received. ${JSON.stringify(msg)}`);
      return;
    }
    delete this.pendingMessages[msg.id];
    if (msg.error) {
      pendingMessage.rej(msg.error);
    } else {
      // Response will likely include just "void", so falsy value
      // still should mean that it is resolved
      pendingMessage.res(msg.response!);
    }
  }
}
