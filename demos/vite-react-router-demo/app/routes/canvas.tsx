import SmelterCanvasOutput from "~/components/SmelterCanvasOutput";
import type { Route } from "./+types/home";
import { useSmelter } from "~/hooks/useSmelter";
import { useCallback, useState } from "react";
import { setWasmBundleUrl } from "@swmansion/smelter-web-wasm";
import { InputStream, Tiles, useInputStreams, Text, View } from "@swmansion/smelter";

export function meta({ }: Route.MetaArgs) {
  return [
    { title: "Canvas example" },
  ];
}

setWasmBundleUrl('/assets/smelter.wasm');

const CAMERA_ID = 'camera';
const SCREEN_SHARE_ID = 'screen';

export default function CanvasPage() {
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
    <div className="w-full border-gray-200 p-8">
      <p className="text-white text-4xl pl-0 p-8">Example canvas output</p>
      <button className="bg-red-40 hover:bg-blue-700 text-white font-bold py-2 px-4 m-4 rounded" onClick={toggleCamera}>
        Toggle camera
      </button>
      <button className="bg-red-40 hover:bg-blue-700 text-white font-bold py-2 px-4 m-4 rounded" onClick={toggleScreenShare}>
        Toggle screen capture
      </button>

      <p className="text-white text-xl pl-0 p-8">Canvas: </p>
      <div>
        {
          smelter &&
          <SmelterCanvasOutput smelter={smelter} width={1280} height={720} audio>
            <SmelterComponent />
          </SmelterCanvasOutput>
        }
      </div>
    </div>
  );
}

function SmelterComponent() {
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
