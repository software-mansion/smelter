import type { Logger } from 'pino';

type RequestMessage<Request> = {
  id: string;
  request: Request;
};

type ResponseMessage<Response> = {
  type: 'workerResponse';
  id: string;
  response?: Response;
  error?: Error;
};

type PendingMessage<Response> = {
  res: (response: Response) => void;
  rej: (err: Error) => void;
};

let requestCounter = 1;

export function listenForMessages<Request, Response>(
  port: MessagePort,
  onMessage: (request: Request) => Promise<Response>
) {
  port.onmessage = async (event: MessageEvent<RequestMessage<Request>>) => {
    try {
      const response = await onMessage(event.data.request);
      port.postMessage({
        type: 'workerResponse',
        id: event.data.id,
        response,
      } as ResponseMessage<Response>);
    } catch (error: any) {
      port.postMessage({
        type: 'workerResponse',
        id: event.data.id,
        error,
      } as ResponseMessage<Response>);
    }
  };
}

export class AsyncMessagePort<Request, Response> {
  private port: MessagePort;
  private pendingMessages: Record<string, PendingMessage<Response>> = {};
  private logger: Logger;

  constructor(port: MessagePort, logger: Logger) {
    this.logger = logger;
    this.port = port;
    this.port.onmessage = (event: MessageEvent<ResponseMessage<Response>>) => {
      if (event.data.type === 'workerResponse') {
        this.handleResponse(event.data);
      }
    };
  }

  public async postMessage(request: Request, transferable?: Transferable[]): Promise<Response> {
    const requestId = String(requestCounter);
    requestCounter += 1;

    const pendingMessage: PendingMessage<Response> = {} as any;
    const responsePromise = new Promise<Response>((res, rej) => {
      pendingMessage.res = res;
      pendingMessage.rej = rej;
    });
    this.pendingMessages[requestId] = pendingMessage;

    if (transferable) {
      this.port.postMessage({ id: requestId, request }, transferable);
    } else {
      this.port.postMessage({ id: requestId, request });
    }
    return responsePromise;
  }

  public terminate() {
    this.port.close();
  }

  private handleResponse(msg: ResponseMessage<Response>) {
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
