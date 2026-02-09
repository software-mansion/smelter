FROM node:24

COPY . /project

WORKDIR /project

RUN npm install -g pnpm;
RUN pnpm install && pnpm build

ENTRYPOINT ["node", "./dist/index.js"]
