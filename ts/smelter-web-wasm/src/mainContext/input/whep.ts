import { sleep } from '../../utils';
import type { Input, RegisterInputResult } from '../input';
import type { InstanceContext } from '../instance';

type WhepInputOptions = {
  ctx: InstanceContext;
  pc: RTCPeerConnection;

  locationUrl: string;
  bearerToken?: string;

  audioElement?: HTMLAudioElement;
};

export class WhepInput implements Input {
  private ctx: InstanceContext;
  private pc: RTCPeerConnection;
  private inputId: string;

  private locationUrl: string;
  private bearerToken?: string;

  private audioElement?: HTMLAudioElement;

  constructor(inputId: string, options: WhepInputOptions) {
    this.inputId = inputId;
    this.ctx = options.ctx;
    this.pc = options.pc;

    this.locationUrl = options.locationUrl;
    this.bearerToken = options.bearerToken;

    this.audioElement = options.audioElement;
  }

  public async terminate(): Promise<void> {
    this.ctx.logger.debug('Terminate WHEP connection.');
    try {
      await fetch(this.locationUrl, {
        method: 'DELETE',
        mode: 'cors',
        headers: {
          ...(this.bearerToken ? { authorization: `Bearer ${this.bearerToken}` } : {}),
        },
      });
    } catch (err: any) {
      // Some services like Twitch do not implement DELETE endpoint
      this.ctx.logger.debug({ err });
    }
    this.pc?.close();

    if (this.audioElement) {
      this.audioElement.pause();
      this.audioElement.srcObject = null;
    }

    await this.ctx.audioMixer.removeInput(this.inputId);
  }
}

export async function handleRegisterWhepInput(
  ctx: InstanceContext,
  inputId: string,
  endpointUrl: string,
  bearerToken?: string
): Promise<RegisterInputResult> {
  const inputStream = new MediaStream();

  const pc = new RTCPeerConnection({
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
    bundlePolicy: 'max-bundle',
  });

  let receivedTracks: MediaStreamTrack[] = [];

  pc.addTransceiver('audio', { direction: 'recvonly' });
  pc.addTransceiver('video', { direction: 'recvonly' });

  pc.ontrack = event => {
    const track = event.track;
    ctx.logger.debug(`Received track:  ${track.kind}`);
    inputStream.addTrack(track);
    receivedTracks.push(track);
  };

  await new Promise<void>(res => {
    pc.addEventListener('negotiationneeded', () => res(), { once: true });
  });

  const locationUrl = await establishWhepConnection(pc, endpointUrl, bearerToken);
  await waitForTracks(receivedTracks);

  const videoTrack = inputStream.getVideoTracks()[0];
  const audioTrack = inputStream.getAudioTracks()[0];

  const transferable: Transferable[] = [];
  let videoStream: ReadableStream<VideoFrame> | undefined;

  if (videoTrack) {
    // @ts-ignore
    const processor = new MediaStreamTrackProcessor({ track: videoTrack });
    videoStream = processor.readable;
    transferable.push(processor.readable);
  }

  let audioElement: HTMLAudioElement | undefined;

  if (audioTrack) {
    ctx.audioMixer.addMediaStreamInput(inputId, audioTrack);

    // Workaround for a Chromium bug where audio remains silent, unless it is played manually
    // https://issues.chromium.org/issues/40094084
    audioElement = new Audio();
    const audioStream = new MediaStream([audioTrack]);
    audioElement.srcObject = audioStream;
    audioElement.muted = true;
    await audioElement.play();
  }

  const input = new WhepInput(inputId, {
    ctx,
    pc,
    bearerToken,
    locationUrl,
    audioElement,
  });

  return {
    input,
    workerMessage: [
      {
        type: 'registerInput',
        inputId,
        input: {
          type: 'stream',
          videoStream,
        },
      },
      transferable,
    ],
  };
}

async function waitForTracks(tracks: MediaStreamTrack[]): Promise<void> {
  const maxWaitMs = 1000;
  const pollIntervalMs = 100;
  const startTimestamp = Date.now();

  while (tracks.length < 1 && Date.now() - startTimestamp < maxWaitMs) {
    await sleep(pollIntervalMs);
  }

  if (tracks.length === 0) {
    throw new Error('No tracks received within timeout.');
  }
}

async function establishWhepConnection(
  pc: RTCPeerConnection,
  endpoint: string,
  token?: string
): Promise<string> {
  await pc.setLocalDescription(await pc.createOffer());

  const offer = await gatherICECandidates(pc);
  if (!offer) {
    throw Error('failed to gather ICE candidates for offer');
  }

  let { sdp: sdpAnswer, location } = await postSdpOffer(endpoint, offer.sdp, token);
  await pc.setRemoteDescription(new RTCSessionDescription({ type: 'answer', sdp: sdpAnswer }));
  return location ?? endpoint;
}

async function gatherICECandidates(
  peerConnection: RTCPeerConnection
): Promise<RTCSessionDescription | null> {
  return new Promise<RTCSessionDescription | null>(res => {
    setTimeout(function () {
      res(peerConnection.localDescription);
    }, 2000);

    peerConnection.onicegatheringstatechange = (_ev: Event) => {
      if (peerConnection.iceGatheringState === 'complete') {
        res(peerConnection.localDescription);
      }
    };
  });
}

async function postSdpOffer(
  endpoint: string,
  sdpOffer: string,
  token?: string
): Promise<{ sdp: string; location: string }> {
  const response = await fetch(endpoint, {
    method: 'POST',
    mode: 'cors',
    headers: {
      'content-type': 'application/sdp',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: sdpOffer,
  });

  if (response.status === 201) {
    return {
      sdp: await response.text(),
      location: getLocationFromHeader(response.headers, endpoint),
    };
  } else {
    const errorMessage = await response.text();
    throw new Error(errorMessage);
  }
}

function getLocationFromHeader(headers: Headers, endpoint: string): string {
  const locationHeader = headers.get('Location');
  if (!locationHeader) {
    // e.g. Twitch CORS blocks access to Location header, so in this case let's assume that
    // location is under the same URL.
    return endpoint;
  }

  return new URL(locationHeader, endpoint).toString();
}
