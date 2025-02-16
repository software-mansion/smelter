import StreamPage from "~/stream/StreamPage";
import type { Route } from "./+types/home";

export function meta({}: Route.MetaArgs) {
  return [
    { title: "Streamer example" },
  ];
}

export default function Stream() {
  return <StreamPage />;
}
