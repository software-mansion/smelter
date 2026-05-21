# Sandbox

Testing playground for Smelter. The Node.js app runs locally and connects to a
Smelter server deployed on `puffer.fishjam.io`.

## Deploy Smelter server

Commits all local changes, pushes the current branch, and rebuilds the server
on the remote machine. After deploy it streams server logs.

```bash
pnpm push
```

To restart the container without rebuilding:

```bash
pnpm push:restart
```

## Run the app (separate terminal)

Set the authorization token for the remote Smelter API and start the app:

```bash
export DEMO_AUTH_HEADER="Bearer <token>"
pnpm start
```
