# Smelter demos

## `@swmansion/smelter-web-client`

Following examples show how to control Smelter server instance from the browser. In those examples,
browser will execute React code via the `@swmansion/smelter-web-client` package and send scene update
requests to the Smelter server

#### `vite-web-client`

Project generated with `pnpm create vite` from `React`+`SWC` template.

To run this project:
- Start smelter server (e.g. binary from https://github.com/smelter-labs/smelter-rc/releases/tag/62d73800).
- Go to `./vite-web-client`.
- Run `pnpm install && pnpm dev`.

## `@swmansion/smelter-web-wasm`

Following examples show how you can use Smelter to render video inside the browser. All of them
do not require any additional infrastructure, and are fully self-contained.

#### `vite-web-wasm`

Project generated with `pnpm create vite` from `React`+`SWC` template.

Go to `./vite-web-wasm` and run `pnpm install && pnpm dev`.

#### `vite-react-router-web-wasm`

Project generated with `pnpm create vite` from React Router template. It includes
following pages:
- In-browser Smelter that renders on canvas. You can add camera and screen share to the scene.
- In-browser Smelter that streams output over WebRTC (WHIP protocol) and displays preview using
in a `<video />` tag. You can add camera and screen share to the scene.

Go to `./vite-react-router-web-wasm` and run `pnpm install && pnpm dev`.

##### `nextjs-web-wasm`

The project was generated using `pnpm create next-app@14.2.24`. Next.js version 14.2.24 was chosen to ensure compatibility with the version of React used in Smelter, as newer Next.js releases are not fully compatible.

Go to `./nextjs-demo` and run `pnpm install && pnpm dev`.
