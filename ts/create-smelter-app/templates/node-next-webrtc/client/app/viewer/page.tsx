import Link from "next/link";
import WhepClientVideo from "@/components/WhepClientVideo";

const WHEP_URL = "http://127.0.0.1:9000/whep/output";

export default function ViewerPage() {
  return (
    <div className="page">
      <header className="header">
        <div className="container flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Link href="/" className="back-link">← Back</Link>
            <span className="text-border">|</span>
            <h1 className="font-medium">Viewer</h1>
          </div>
          <div className="flex items-center gap-2">
            <span className="status-dot bg-accent animate-pulse" />
            <span className="text-sm text-muted">Watching</span>
          </div>
        </div>
      </header>

      <main className="container py-8">
        <div className="card overflow-hidden">
          <div className="video-container">
            <WhepClientVideo
              url={WHEP_URL}
              poster="https://placehold.co/1920x1080/0f0f0f/27272a?text=Waiting..."
              playsInline autoPlay controls
              className="video"
            />
          </div>
        </div>
        <p className="mt-6 text-sm text-muted text-center">
          Stream will appear when a broadcaster starts streaming
        </p>
      </main>
    </div>
  );
}
