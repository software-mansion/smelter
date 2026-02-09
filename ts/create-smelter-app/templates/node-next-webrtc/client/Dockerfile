FROM node:24

COPY . /project

WORKDIR /project

ENV CI=true

RUN npm install -g pnpm;
RUN pnpm install && pnpm build

ENTRYPOINT ["pnpm", "start"]
