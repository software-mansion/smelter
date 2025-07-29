### Install dependencies

```
pnpm i
```

### Run broadcast-box

Smelter instance streams WHIP to localhost:8080. To preview the stream you can run broadcast box inside a docker container with.

```
docker run -d -e UDP_MUX_PORT=8080 -e NETWORK_TEST_ON_START=false -e NAT_1_TO_1_IP=127.0.0.1 -p 8080:8080 -p 8080:8080/udp seaduboi/broadcast-box
```

### Run the demo

```
pnpm run dev
```
