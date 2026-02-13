"use client";

import PageHeader from "@/components/PageHeader";
import WhepClientVideo from "@/components/WhepClientVideo";
import { WhipClient } from "@/utils/whip-client";
import { useRef, useState } from "react";

// Base url of a WHIP/WHEP server. By default, Smelter exposes this server on
// port 9000, but the value can be changed via SMELTER_WHIP_WHEP_SERVER_PORT
// environment variable.
const SMELTER_WHIP_WHEP_URL = new URL("http://127.0.0.1:9000");

// API of Node.js server from `/server` directory.
const BACKEND_URL = new URL("http://127.0.0.1:3001");

const WHIP_AUTH_TOKEN = "example_token";

type Connection = "camera" | "screen-share" | "none";

export default function StreamerPage() {
  const clientRef = useRef<WhipClient>(new WhipClient());
  const [connection, setConnection] = useState<Connection>("none");
  const [showInstructions, setShowInstructions] = useState(true);

  const toggleCamera = async () => {
    setConnection("none");
    await clientRef.current.close();
    if (connection !== "camera") {
      const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
      await clientRef.current.connect(stream, new URL("/whip/input", SMELTER_WHIP_WHEP_URL), WHIP_AUTH_TOKEN);
      setConnection("camera");
    }
  };

  const toggleScreenShare = async () => {
    setConnection("none");
    await clientRef.current.close();
    if (connection !== "screen-share") {
      const stream = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: true });
      await clientRef.current.connect(stream, new URL("/whip/input", SMELTER_WHIP_WHEP_URL), WHIP_AUTH_TOKEN);
      setConnection("screen-share");
    }
  };

  const toggleInstructions = async () => {
    setShowInstructions(!showInstructions);
    await fetch(new URL("/layout-update", BACKEND_URL), {
      method: "POST",
      mode: "cors",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ showInstructions: !showInstructions }),
    });
  };

  return (
    <div className="min-h-screen bg-background text-foreground font-sans">
      <PageHeader
        title="Streamer"
        statusDot={connection === "none" ? "bg-muted" : "bg-red-500 animate-pulse"}
        statusText={connection === "none" ? "Not streaming" : connection === "camera" ? "Camera" : "Screen"}
      />

      <main className="max-w-6xl mx-auto px-6 py-8">
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2">
            <div className="bg-card border border-border rounded-lg overflow-hidden">
              <div className="aspect-video bg-black">
                <WhepClientVideo
                  url={new URL("/whep/output", SMELTER_WHIP_WHEP_URL).toString()}
                  poster="https://placehold.co/1920x1080/0f0f0f/27272a?text=Preview"
                  playsInline autoPlay controls
                  className="w-full h-full object-contain"
                />
              </div>
            </div>
            <p className="mt-3 text-sm text-muted text-center">Output preview</p>
          </div>

          <div className="space-y-4">
            <div className="bg-card border border-border rounded-lg p-5">
              <h2 className="text-sm font-medium text-muted uppercase tracking-wide mb-4">Source</h2>
              <div className="space-y-3">
                <button onClick={toggleScreenShare} className={`w-full py-3 px-4 rounded-lg font-medium transition-all ${connection === "screen-share" ? "bg-red-500/20 text-red-400 border border-red-500/30 hover:bg-red-500/30" : "bg-accent hover:bg-accent-hover text-white"}`}>
                  {connection === "screen-share" ? "Stop" : "Start"} Screen Share
                </button>
                <button onClick={toggleCamera} className={`w-full py-3 px-4 rounded-lg font-medium transition-all ${connection === "camera" ? "bg-red-500/20 text-red-400 border border-red-500/30 hover:bg-red-500/30" : "bg-accent hover:bg-accent-hover text-white"}`}>
                  {connection === "camera" ? "Stop" : "Start"} Camera
                </button>
              </div>
            </div>

            <div className="bg-card border border-border rounded-lg p-5">
              <h2 className="text-sm font-medium text-muted uppercase tracking-wide mb-4">Overlay</h2>
              <label className="flex items-center justify-between cursor-pointer">
                <span>Show instructions</span>
                <div
                  className={`relative w-11 h-6 rounded-full transition-colors ${showInstructions ? "bg-accent" : "bg-border"}`}
                  onClick={toggleInstructions}
                >
                  <div className={`absolute top-1 w-4 h-4 bg-white rounded-full transition-transform ${showInstructions ? "translate-x-6" : "translate-x-1"}`} />
                </div>
              </label>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
