import type { Route } from "./+types/home";
import { Home as HomeComponent } from "../home/home";

export function meta({ }: Route.MetaArgs) {
  return [
    { title: "Smelter" },
  ];
}

export default function Home() {
  return <HomeComponent />;
}
