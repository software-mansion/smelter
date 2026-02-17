# Overview

This application is built from 3 components:
- Smelter server
- Node.js process that controls the Smelter server.
- Next.js app:
  - Streams camera or screen share to Smelter over WHIP.
  - Displays modified stream received from Smelter (broadcasted over WHEP) 
  - Controls video layout via HTTP API of the Node.js process.

# Usage

## Development

#### Step 1: Start Node.js server

  In **`./server`** run:

```sh
pnpm install && pnpm start
```

Node.js server will automatically start a Smelter server and connect to it.

#### Step 2: Start Next.js app

In **`./client`** (in a separate terminal) run:

```sh
pnpm install && pnpm dev
```

Open `localhost:3000` in your browser.

## Development (with Smelter inside Docker)

> Running Smelter inside a Docker container without GPU acceleration will be significantly slower. Check out `compose.yml`
  to learn how to enable it on Nvidia and AMD cards.

#### Step 1: Start Smelter server

In root directory run:

```sh
docker compose up
```

#### Step 2: Start Node.js server

In **`./server`** (in a separate terminal) run:

```sh
pnpm install && \
SMELTER_INSTANCE_URL=http://localhost:8081 pnpm start
```

This server will manage Smelter instance created in previous step.

#### Step 3: Start Next.js app

In **`./client`** (in a separate terminal) run:

```sh
pnpm install && pnpm dev
```

Open `localhost:3000` in your browser.

## Production

Run `COMPOSE_PROFILES=prod docker compose up`
