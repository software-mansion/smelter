export class WhepClient {
  private pc?: RTCPeerConnection;
  private locationUrl?: string;
  private bearerToken?: string;

  public async connect(
    url: string,
    bearerToken?: string
  ): Promise<MediaStream> {
    if (this.pc) {
      await this.close();
    }

    const pc = new RTCPeerConnection({
      iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
      bundlePolicy: 'max-bundle',
    });
    this.pc = pc;

    pc.addTransceiver('audio', { direction: 'recvonly' });
    pc.addTransceiver('video', { direction: 'recvonly' });

    const stream = new MediaStream();
    const onTrackPromise = new Promise<void>((res) => {
      // just in case so we don
      pc.ontrack = event => {
        stream.addTrack(event.track);
        const expectedTracks = pc.getTransceivers().filter(t =>
          t.mid !== null && !['inactive', 'stopped'].includes(t.currentDirection ?? '')
        );
        if (stream.getTracks().length >= expectedTracks.length) {
          res()
        }
        // Just to make sure that we will block after first
        // track.
        setTimeout(() => { res() }, 1000);
      };
    })

    await pc.setLocalDescription(await pc.createOffer());
    const offer = await waitForIceCandidates(pc);
    if (!offer) {
      throw Error('failed to gather ICE candidates for offer');
    }

    const { sdpAnswer, locationHeader } = await postSdpOffer(url, offer.sdp, bearerToken);
    await pc.setRemoteDescription(new RTCSessionDescription({ type: 'answer', sdp: sdpAnswer }));
    this.locationUrl = locationHeader ?? url;

    await onTrackPromise;
    return stream
  }

  public async close() {
    if (!this.pc) {
      return
    }

    this.pc.close();
    delete this.pc;

    if (this.locationUrl) {
      try {
        await fetch(this.locationUrl, {
          method: 'DELETE',
          mode: 'cors',
          headers: {
            'content-type': 'application/sdp',
            ...(this.bearerToken ? { authorization: `Bearer ${this.bearerToken}` } : {}),
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
  endpoint: string,
  sdpOffer: string,
  token?: string
): Promise<{ sdpAnswer: string; locationHeader: string }> {
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
      sdpAnswer: await response.text(),
      locationHeader: getLocationFromHeader(response.headers, endpoint),
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
