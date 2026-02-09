export class WhipClient {
  private pc?: RTCPeerConnection;
  private stream?: MediaStream;
  private locationUrl?: string;
  private bearerToken?: string;

  public async connect(
    stream: MediaStream,
    url: string | URL,
    bearerToken?: string
  ): Promise<void> {
    if (this.pc) {
      await this.close();
    }
    this.stream = stream;
    this.bearerToken = bearerToken;
    const videoTrack = stream.getVideoTracks()[0];
    const audioTrack = stream.getAudioTracks()[0];

    const pc = new RTCPeerConnection({
      iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
      bundlePolicy: 'max-bundle',
    });
    this.pc = pc;

    if (videoTrack) {
      pc.addTransceiver(videoTrack, {
        direction: 'sendonly',
        sendEncodings: [
          {
            priority: 'high',
            networkPriority: 'high',
            scaleResolutionDownBy: 1.0,
          },
        ]
      });
    }
    if (audioTrack) {
      pc.addTransceiver(audioTrack, { direction: 'sendonly' });
    }

    await pc.setLocalDescription(await pc.createOffer());

    const offer = await waitForIceCandidates(pc);
    if (!offer) {
      throw Error('Failed to gather ICE candidates for offer');
    }

    const { sdpAnswer, locationHeader } = await postSdpOffer(url, offer.sdp, bearerToken);
    await pc.setRemoteDescription(new RTCSessionDescription({ type: 'answer', sdp: sdpAnswer }));
    this.locationUrl = locationHeader ?? url;
  }

  public async close() {
    if (!this.pc) {
      return
    }
    const pc = this.pc;
    delete this.pc;
    const stream = this.stream;
    delete this.stream;
    const locationUrl = this.locationUrl;
    const bearerToken = this.bearerToken;

    pc.close();

    if (stream) {
      stream.getTracks().forEach(track => track.stop())
    }

    if (locationUrl) {
      try {
        await fetch(locationUrl, {
          method: 'DELETE',
          mode: 'cors',
          headers: {
            ...(bearerToken ? { authorization: `Bearer ${bearerToken}` } : {}),
          },
        });
      } catch (err) {
        console.warn("Failed to delete a WHEP session.", err)
      }
    }
  }
}

async function waitForIceCandidates(
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
  url: URL | string,
  sdpOffer: string,
  token?: string
): Promise<{ sdpAnswer: string; locationHeader: string }> {
  const response = await fetch(url, {
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
      sdpAnswer: await response.text(),
      locationHeader: getLocationFromHeader(response.headers, url.toString()),
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
