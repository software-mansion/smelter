import CanvasPage from "~/canvas/CanvasPage";
import type { Route } from "./+types/home";

export function meta({ }: Route.MetaArgs) {
  return [
    { title: "Canvas example" },
  ];
}

export default function Canvas() {
  return <CanvasPage />;
}
