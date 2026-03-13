import type { ApiClient, RegisterInputResponse } from './api';
import type { InputRef, RegisterInput } from './api/input';

export function newInputHandle(
  inputRef: InputRef,
  api: ApiClient,
  response: RegisterInputResponse,
  kind: RegisterInput['type']
) {
  if (kind == 'whip_server') {
    return new WhipInputHandle(inputRef, api, response);
  } else {
    return new InputHandle(inputRef, api, response);
  }
}

export class InputHandle {
  private inputRef: InputRef;
  private api: ApiClient;
  protected registerResponse: RegisterInputResponse;

  constructor(inputRef: InputRef, api: ApiClient, response: RegisterInputResponse) {
    this.inputRef = inputRef;
    this.api = api;
    this.registerResponse = response;
  }

  public get videoDurationMs(): number | undefined {
    return this.registerResponse.video_duration_ms;
  }

  public get audioDurationMs(): number | undefined {
    return this.registerResponse.audio_duration_ms;
  }

  public async pause(): Promise<void> {
    await this.api.updateInput(this.inputRef, { pause: true });
  }

  public async resume(): Promise<void> {
    await this.api.updateInput(this.inputRef, { pause: false });
  }
}

export class WhipInputHandle extends InputHandle {
  public get endpointRoute(): string | undefined {
    return this.registerResponse.endpoint_route;
  }

  public get bearerToken(): string | undefined {
    return this.registerResponse.bearer_token;
  }
}
