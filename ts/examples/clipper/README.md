### Start the server

```
pnpm run dev
```

### Update DB schema

```
pnpm drizzle-kit push
```

### Generating localhost certificate

```
mkcert localhost
```

### Running Broadcast Box

```
docker run -e UDP_MUX_PORT=8080 -e NETWORK_TEST_ON_START=false -e NAT_1_TO_1_IP=127.0.0.1 -p 8080:8080 -p 8080:8080/udp -d seaduboi/broadcast-box
```
