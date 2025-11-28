import React, { useCallback, useEffect, useState } from 'react';
import type Smelter from '@swmansion/smelter-web-client';

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type SmelterWhepOutputProps = {
  smelter: Smelter;
  children: React.ReactElement;
} & VideoProps;

export default function SmelterWhepOutput(props: SmelterWhepOutputProps) {
  const { children, smelter, ...videoProps } = props;
  const [videoElement, setVideoElement] = useState<HTMLVideoElement | null>(null);

  const videoRef = useCallback(async (updatedVideo: HTMLVideoElement | null) => {
    setVideoElement(updatedVideo);
  }, []);

  useEffect(
    () => {
      if (!videoElement) {
        return;
      }

      const outputId = getNewOutputId();
      const promise = (async () => {
        const output = await smelter.registerOutput(outputId, children, {
          type: 'whep_server',
          bearerToken: "example",
          video: {
            encoder: {
              type: "ffmpeg_h264",
              preset: "ultrafast"
            },
            resolution: {
              width: 1920,
              height: 1080,
            }
          },
          audio: {
            encoder: {
              type: "opus"
            }
          },
        });
        console.log({ output })

        const stream = await connectToWhepServer(`http://127.0.0.1:9000${output.endpointRoute}`, "example");
        // eslint-disable-next-line
        (videoElement as any).srcObject = stream;
        await videoElement.play()
      })();

      return () => {
        void promise
          .catch(console.error)
          .then(() => smelter.unregisterOutput(outputId))
          .catch(console.error);
      };
    },
    // eslint-disable-next-line
    [smelter, videoElement]
  );

  return <video ref={videoRef} {...videoProps} />;
}

async function connectToWhepServer(
  url: string,
  bearerToken?: string
): Promise<MediaStream> {
  const stream = new MediaStream();

  const pc = new RTCPeerConnection({
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
    bundlePolicy: 'max-bundle',
  });

  pc.addTransceiver('audio', { direction: 'recvonly' });
  pc.addTransceiver('video', { direction: 'recvonly' });

  const onTrackPromise = new Promise<void>((res) => {
    pc.ontrack = event => {
      console.log('Received track', event.track);
      stream.addTrack(event.track);
      if (stream.getAudioTracks().length >= 1 && stream.getVideoTracks().length >= 1) {
        res()
      }
    };
  })

  await new Promise<void>(res => {
    pc.addEventListener('negotiationneeded', () => res(), { once: true });
  });

  const locationUrl = await establishWhepConnection(pc, url, bearerToken);
  console.log({ locationUrl })
  await onTrackPromise

  return stream
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

  const { sdp: sdpAnswer, location } = await postSdpOffer(endpoint, offer.sdp, token);
  await pc.setRemoteDescription(new RTCSessionDescription({ type: 'answer', sdp: sdpAnswer }));
  return location ?? endpoint;
}

async function gatherICECandidates(
  peerConnection: RTCPeerConnection
): Promise<RTCSessionDescription | null> {
  return new Promise<RTCSessionDescription | null>(res => {
    setTimeout(function() {
      res(peerConnection.localDescription);
    }, 2000);

    peerConnection.onicegatheringstatechange = () => {
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

const getNewOutputId = (() => {
  let counter = 1;
  return () => {
    const outputId = `output-${counter}`;
    counter += 1;
    return outputId;
  };
})();
