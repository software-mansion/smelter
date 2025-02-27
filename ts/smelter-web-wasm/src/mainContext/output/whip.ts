import type { RegisterOutput } from '../../workerApi';
import type { Output, RegisterOutputResult, RegisterWasmWhipOutput } from '../output';
import type { InstanceContext } from '../instance';

type PeerConnectionOptions = {
  ctx: InstanceContext;
  pc: RTCPeerConnection;

  locationUrl: string;
  bearerToken?: string;

  canvasStream?: MediaStream;
};

type VideoTrackResult = {
  workerMessage: RegisterOutput['video'];
  canvasStream: MediaStream;
  track: MediaStreamTrack;
  transferable: Transferable[];
};

type AudioTrackResult = {
  track: MediaStreamTrack;
  transferable: Transferable[];
};

export class WhipOutput implements Output {
  private ctx: InstanceContext;
  private pc: RTCPeerConnection;
  private outputId: string;

  private locationUrl: string;
  private bearerToken?: string;

  private canvasStream?: MediaStream;

  constructor(outputId: string, options: PeerConnectionOptions) {
    this.outputId = outputId;
    this.ctx = options.ctx;
    this.pc = options.pc;

    this.locationUrl = options.locationUrl;
    this.bearerToken = options.bearerToken;

    this.canvasStream = options.canvasStream;
  }

  public async terminate(): Promise<void> {
    this.ctx.logger.debug('Terminate WHIP connection.');
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
    this.canvasStream?.getTracks().forEach(track => track.stop());
    await this.ctx.audioMixer.removeOutput(this.outputId);
  }
}

export async function handleRegisterWhipOutput(
  ctx: InstanceContext,
  outputId: string,
  request: RegisterWasmWhipOutput
): Promise<RegisterOutputResult> {
  const outputStream = new MediaStream();
  const pc = new RTCPeerConnection({
    iceServers: request.iceServers || [{ urls: 'stun:stun.l.google.com:19302' }],
    bundlePolicy: 'max-bundle',
  });
  const negotiationNeededPromise = new Promise<void>(res => {
    pc.addEventListener('negotiationneeded', () => {
      res();
    });
  });

  const videoResult = await handleVideo(ctx, pc, outputId, request);
  const audioResult = await handleAudio(ctx, pc, outputId, request);

  await negotiationNeededPromise;
  const locationUrl = await establishWhipConnection(pc, request.endpointUrl, request.bearerToken);

  const output = new WhipOutput(outputId, {
    ctx,
    pc,
    bearerToken: request.bearerToken,
    locationUrl,
    canvasStream: videoResult?.canvasStream,
  });

  if (videoResult) {
    outputStream.addTrack(videoResult.track);
  }
  if (audioResult) {
    outputStream.addTrack(audioResult.track);
  }

  return {
    output,
    result: {
      type: 'web-wasm-whip',
      stream: outputStream,
    },
    workerMessage: [
      {
        type: 'registerOutput',
        outputId,
        output: {
          type: 'stream',
          video: videoResult?.workerMessage,
        },
      },
      [...(videoResult?.transferable ?? []), ...(audioResult?.transferable ?? [])],
    ],
  };
}

async function handleVideo(
  ctx: InstanceContext,
  pc: RTCPeerConnection,
  _outputId: string,
  request: RegisterWasmWhipOutput
): Promise<VideoTrackResult | undefined> {
  if (!request.video || !request.initial.video) {
    return undefined;
  }

  const canvas = document.createElement('canvas');
  canvas.width = request.video.resolution.width;
  canvas.height = request.video.resolution.height;
  const canvasStream = canvas.captureStream(ctx.framerate.num / ctx.framerate.den);
  const track = canvasStream.getVideoTracks()[0];
  const offscreen = canvas.transferControlToOffscreen();

  await track.applyConstraints({
    width: { exact: request.video.resolution.width },
    height: { exact: request.video.resolution.height },
    frameRate: { ideal: ctx.framerate.num / ctx.framerate.den },
  });

  const videoSender = pc.addTransceiver(track, {
    direction: 'sendonly',
    sendEncodings: [
      {
        maxBitrate: request.video?.maxBitrate,
        priority: 'high',
        networkPriority: 'high',
        scaleResolutionDownBy: 1.0,
      },
    ],
  });

  const params = videoSender.sender.getParameters();
  params.degradationPreference = 'maintain-resolution';
  await videoSender.sender.setParameters(params);

  return {
    workerMessage: {
      resolution: request.video.resolution,
      initial: request.initial.video,
      canvas: offscreen,
    },
    canvasStream,
    track,
    transferable: [offscreen],
  };
}

async function handleAudio(
  ctx: InstanceContext,
  pc: RTCPeerConnection,
  outputId: string,
  request: RegisterWasmWhipOutput
): Promise<AudioTrackResult | undefined> {
  if (!request.audio || !request.initial.audio) {
    return undefined;
  }
  const track = ctx.audioMixer.addMediaStreamOutput(outputId);
  pc.addTransceiver(track, { direction: 'sendonly' });
  return {
    track,
    transferable: [],
  };
}

async function establishWhipConnection(
  pc: RTCPeerConnection,
  endpoint: string,
  token?: string
): Promise<string> {
  await pc.setLocalDescription(await pc.createOffer());

  const offer = await gatherICECandidates(pc);
  if (!offer) {
    throw Error('failed to gather ICE candidates for offer');
  }

  /**
   * This response contains the server's SDP offer.
   * This specifies how the client should communicate,
   * and what kind of media client and server have negotiated to exchange.
   */
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
