import type { Logger } from 'pino';
import type {
  MainThreadHandle,
  WorkerEvent,
  WorkerHandle,
  WorkerMessage,
  WorkerResponse,
} from '../../workerApi';

type RequestMessage = {
  id: string;
  request: WorkerMessage;
};

type ResponseMessage = {
  type: 'workerResponse';
  id: string;
  response?: WorkerResponse;
  error?: Error;
};

type EventMessage = {
  type: 'workerEvent';
  event: WorkerEvent;
};

type PendingMessage = {
  res: (response: WorkerResponse) => void;
  rej: (err: Error) => void;
};

let requestCounter = 1;

export function registerWorkerEntrypoint(
  onMessage: (handle: MainThreadHandle, request: WorkerMessage) => Promise<WorkerResponse>
) {
  const handle = new Handle();
  self.onmessage = async (event: MessageEvent<RequestMessage>) => {
    try {
      const response = await onMessage(handle, event.data.request);
      self.postMessage({
        type: 'workerResponse',
        id: event.data.id,
        response,
      } as ResponseMessage);
    } catch (error: any) {
      self.postMessage({
        type: 'workerResponse',
        id: event.data.id,
        error,
      } as ResponseMessage);
    }
  };
}

export class DedicatedWorker implements WorkerHandle {
  private worker: Worker;
  private pendingMessages: Record<string, PendingMessage> = {};
  private onEvent: (event: WorkerEvent) => void;
  private logger: Logger;

  constructor(worker: Worker, onEvent: (event: WorkerEvent) => void, logger: Logger) {
    this.logger = logger;
    this.worker = worker;
    this.worker.onmessage = (event: MessageEvent<ResponseMessage | EventMessage>) => {
      if (event.data.type === 'workerEvent') {
        this.handleEvent(event.data.event);
      } else if (event.data.type === 'workerResponse') {
        this.handleResponse(event.data);
      }
    };
    this.onEvent = onEvent;
  }

  public async postMessage(
    request: WorkerMessage,
    transferable?: Transferable[]
  ): Promise<WorkerResponse> {
    const requestId = String(requestCounter);
    requestCounter += 1;

    const pendingMessage: PendingMessage = {} as any;
    const responsePromise = new Promise<WorkerResponse>((res, rej) => {
      pendingMessage.res = res;
      pendingMessage.rej = rej;
    });
    this.pendingMessages[requestId] = pendingMessage;

    if (transferable) {
      this.worker.postMessage({ id: requestId, request }, transferable);
    } else {
      this.worker.postMessage({ id: requestId, request });
    }
    return responsePromise;
  }

  public async terminate() {
    this.worker.terminate();
  }

  private handleEvent(event: WorkerEvent) {
    this.onEvent(event);
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

class Handle implements MainThreadHandle {
  public async postEvent(event: WorkerEvent) {
    self.postMessage({ type: 'workerEvent', event });
  }
}
