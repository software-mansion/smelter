## Running in development

### 1. Generate localhost certificates

To run https in development you need to generate self-signed certificates. This can be done with [mkcert](https://github.com/FiloSottile/mkcert)

```
mkdir .certs && mkcert -cert-file .certs/localhost.pem -key-file .certs/localhost-key.pem localhost 127.0.0.1
```

### 2. Push libSQL database schema

Clipper stores processed clip requests in local libSQL database. To initialize the DB run

```
pnpm drizzle-kit push
```

You can manage your database schema with drizzle-kit studio

```
pnpm drizzle-kit studio
```

### 3. Prepare output directories

Clipper needs to know where to store output HLS stream and created clips. For dev environment, you can create `.tmp` folder with `hls` and `clips` folders inside

```
mkdir -p .tmp/hls .tmp/clips
```

### 4. Start the server

```
pnpm run dev
```

## Env variables

See `.env.example` for descriptions of available settings.
