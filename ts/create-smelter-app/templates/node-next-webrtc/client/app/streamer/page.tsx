"use client";

import Link from "next/link";
import WhepClientVideo from "@/components/WhepClientVideo";
import { WhipClient } from "@/utils/whip-client";
import { useRef, useState } from "react";

const SMELTER_URL = "http://127.0.0.1:9000";
const BACKEND_URL = "http://127.0.0.1:3001";
const WHIP_AUTH_TOKEN = "example_token";

type Connection = "camera" | "screen-share" | "none";

export default function StreamerPage() {
  const clientRef = useRef<WhipClient>(new WhipClient());
  const [connection, setConnection] = useState<Connection>("none");
  const [showInstructions, setShowInstructions] = useState(true);

  const connect = async (type: "camera" | "screen-share") => {
    setConnection("none");
    await clientRef.current.close();
    if (connection !== type) {
      const stream = await navigator.mediaDevices[
        type === "camera" ? "getUserMedia" : "getDisplayMedia"
      ]({ video: true, audio: true });
      await clientRef.current.connect(stream, new URL("/whip/input", SMELTER_URL), WHIP_AUTH_TOKEN);
      setConnection(type);
    }
  };

  const toggleInstructions = async () => {
    setShowInstructions(!showInstructions);
    await fetch(`${BACKEND_URL}/layout-update`, {
      method: "POST",
      mode: "cors",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ showInstructions: !showInstructions }),
    });
  };

  return (
    <div className="page">
      <header className="header">
        <div className="container flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Link href="/" className="back-link">← Back</Link>
            <span className="text-border">|</span>
            <h1 className="font-medium">Streamer</h1>
          </div>
          <div className="flex items-center gap-2">
            <span className={`status-dot ${connection === "none" ? "bg-muted" : "bg-red-500 animate-pulse"}`} />
            <span className="text-sm text-muted">
              {connection === "none" ? "Not streaming" : connection === "camera" ? "Camera" : "Screen"}
            </span>
          </div>
        </div>
      </header>

      <main className="container py-8">
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2">
            <div className="card overflow-hidden">
              <div className="video-container">
                <WhepClientVideo
                  url={`${SMELTER_URL}/whep/output`}
                  poster="https://placehold.co/1920x1080/0f0f0f/27272a?text=Preview"
                  playsInline autoPlay controls
                  className="video"
                />
              </div>
            </div>
            <p className="mt-3 text-sm text-muted text-center">Output preview</p>
          </div>

          <div className="space-y-4">
            <div className="card p-5">
              <h2 className="section-title">Source</h2>
              <div className="space-y-3">
                <button onClick={() => connect("screen-share")} className={`btn ${connection === "screen-share" ? "btn-danger" : ""}`}>
                  {connection === "screen-share" ? "Stop" : "Start"} Screen Share
                </button>
                <button onClick={() => connect("camera")} className={`btn ${connection === "camera" ? "btn-danger" : ""}`}>
                  {connection === "camera" ? "Stop" : "Start"} Camera
                </button>
              </div>
            </div>

            <div className="card p-5">
              <h2 className="section-title">Overlay</h2>
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
