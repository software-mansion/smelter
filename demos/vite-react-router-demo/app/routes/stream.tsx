import type { Route } from "./+types/home";

import { setWasmBundleUrl } from "@swmansion/smelter-web-wasm";
import { useCallback, useState } from "react";
import SmelterWhipOutput from "~/components/SmelterWhipOutput";
import { useSmelter } from "~/hooks/useSmelter";
import { InputStream, Tiles, Text, useInputStreams, View } from "@swmansion/smelter";

setWasmBundleUrl('/assets/smelter.wasm');

export function meta({ }: Route.MetaArgs) {
  return [
    { title: "Streamer example" },
  ];
}

const CAMERA_ID = 'camera';
const SCREEN_SHARE_ID = 'screen';

export default function StreamPage() {
  const [streamInfo, setStreamInfo] = useState<{ endpointUrl: string, token?: string }>();

  return (
    <div className="w-full border-gray-200 p-8">
      <p className="text-white text-4xl pl-0 p-8">Example canvas output</p>
      {
        streamInfo ? <StreamControls {...streamInfo} /> : <StreamSetup onStreamStart={setStreamInfo} />
      }
    </div>
  );
}

function StreamSetup(props: { onStreamStart(streamInfo: { endpointUrl: string, token?: string }): void }) {
  const [token, setToken] = useState('');
  const [endpointUrl, setEndpointUrl] = useState('');

  const onSubmit = () => {
    props.onStreamStart({
      endpointUrl,
      token: token || undefined
    })
  }
  return (
    <>
      <div className="mb-4">
        <label className="block text-gray-300 text-sm font-bold mb-2">
          URL (e.g. For Twitch use https://g.webrtc.live-video.net:4443/v2/offer)
        </label>
        <input className="shadow appearance-none border rounded w-full py-2 px-3 text-gray-300 leading-tight focus:outline-none focus:shadow-outline" id="username" type="text" placeholder="WHIP endpoint url" onChange={(e) => setEndpointUrl(e.target.value)} />
      </div>
      <div className="mb-6">
        <label className="block text-gray-300 text-sm font-bold mb-2">
          Token (optional)
        </label>
        <input className="shadow appearance-none border rounded w-full py-2 px-3 text-gray-300 mb-3 leading-tight focus:outline-none focus:shadow-outline" id="password" type="password" placeholder="******************" onChange={(e) => setToken(e.target.value)} />
      </div>

      <button className="bg-red-40 hover:bg-red-60 text-white font-bold py-2 px-4 m-4 rounded" onClick={onSubmit}>
        Start stream
      </button>
    </>
  )
}

function StreamControls(props: { endpointUrl: string, token?: string }) {
  const smelter = useSmelter();
  const [camera, setCamera] = useState<boolean>();
  const [screen, setScreen] = useState<boolean>();

  const toggleCamera = useCallback(async () => {
    if (camera) {
      try {
        await smelter?.unregisterInput(CAMERA_ID)
        setCamera(false);
      } catch (err) {
        console.log(err, 'Failed to unregister camera')
      }
    } else {
      try {
        await smelter?.registerInput(CAMERA_ID, { 'type': 'camera' });
        setCamera(true);
      } catch (err) {
        console.log(err, 'Failed to register camera')
      }
    }
  }, [smelter, camera])

  const toggleScreenShare = useCallback(async () => {
    if (screen) {
      try {
        await smelter?.unregisterInput(SCREEN_SHARE_ID)
        setScreen(false);
      } catch (err) {
        console.log(err, 'Failed to unregister screen share input')
      }
    } else {
      try {
        await smelter?.registerInput(SCREEN_SHARE_ID, { 'type': 'screen_capture' });
        setScreen(true);
      } catch (err) {
        console.log(err, 'Failed to register screen share input')
      }
    }
  }, [smelter, screen])

  return (
    <>
      <button className="bg-red-40 hover:bg-red-60 text-white font-bold py-2 px-4 m-4 rounded" onClick={toggleCamera}>
        Toggle camera
      </button>
      <button className="bg-red-40 hover:bg-red-60 text-white font-bold py-2 px-4 m-4 rounded" onClick={toggleScreenShare}>
        Toggle screen capture
      </button>

      <p className="text-white text-xl pl-0 p-8">Canvas: </p>
      <div>
        {
          smelter &&
          <SmelterWhipOutput
            smelter={smelter}
            endpointUrl={props.endpointUrl}
            bearerToken={props.token}
            video={{ resolution: { width: 1270, height: 720 } }}
            audio>
            <SmelterScene />
          </SmelterWhipOutput>
        }
      </div>
    </>
  )
}

function SmelterScene() {
  const inputs = useInputStreams();
  const hasCamera = !!inputs[CAMERA_ID];
  const hasScreenShare = !!inputs[SCREEN_SHARE_ID];
  return (
    <View style={{ backgroundColor: '#302555' }}>
      <Tiles>
        {hasCamera ? <InputStream inputId={CAMERA_ID} /> : undefined}
        {hasScreenShare ? <InputStream inputId={SCREEN_SHARE_ID} /> : undefined}
        {!hasCamera && !hasScreenShare ? <Text style={{ fontSize: 100 }}>No input.{'\n'}Add camera and/or screen share.</Text> : undefined}
      </Tiles>
      <View style={{ bottom: 0, left: 0, height: 50, padding: 20, backgroundColor: '#FFFFFF88' }}>
        <Text style={{ color: 'red', fontSize: 50 }}>Example app</Text>
      </View>
    </View>
  )
}
